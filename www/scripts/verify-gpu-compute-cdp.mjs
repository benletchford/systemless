#!/usr/bin/env node
// Browser GPU compute qualification. Build with `cd www && trunk build`, then
// pass the dist directory as argv[2]. Requires real WebGPU: an unavailable
// adapter fails rather than skips.
//
// Drives the production emulator-worker.js through the desktop play scenarios
// (tests/play/toolbox-showcase*.json) once on the CPU and once with every
// supported QuickDraw/QD3D operation forced through the compute worker. GPU
// captures must match the CPU captures, and GPU request counters must show the
// pixels came from the compute worker rather than a silent CPU fallback. A
// final run injects device loss mid-scenario; the guest must finish on the CPU
// with identical output. Pages are served with www/public/_headers, so the run
// also proves those headers make the page cross-origin isolated.
import {spawn} from 'node:child_process';
import {existsSync,readFileSync,readdirSync} from 'node:fs';
import {mkdtemp,rm} from 'node:fs/promises';
import {createServer} from 'node:http';
import {tmpdir} from 'node:os';
import {join,resolve,extname} from 'node:path';
import {connect} from './runtime-cdp.mjs';

const dist=resolve(process.argv[2] || 'www/dist');
const runtime=readdirSync(dist).find(name=>/^systemless.*\.js$/.test(name));
const wasm=readdirSync(dist).find(name=>/^systemless.*_bg\.wasm$/.test(name));
if(!runtime || !wasm)throw new Error('Build the browser bundle first');
const repo=new URL('../../',import.meta.url);
const scenario=name=>JSON.parse(readFileSync(new URL('tests/play/'+name,repo),'utf8')).actions;
const headers=Object.fromEntries(readFileSync(new URL('www/public/_headers',repo),'utf8').split('\n')
  .filter(line=>/^\s+\S+:/.test(line)).map(line=>line.trim().split(/:\s*/)));
if(!headers['Cross-Origin-Opener-Policy'] || !headers['Cross-Origin-Embedder-Policy'])throw new Error('Missing isolation headers');
// [name, architecture, scenario, tolerance, request kinds that must reach the GPU]
const cases=[
  ['classic-ui','68k','toolbox-showcase.json',0,['raster']],
  ['native-sprites','ppc','toolbox-showcase-ppc-sprites.json',0,['blit']],
  ['native-3d','ppc','toolbox-showcase-ppc-qd3d.json',8,['qd3d']],
].map(([name,architecture,file,tolerance,kinds])=>({name,architecture,actions:scenario(file),tolerance,kinds}));

const types={'.wasm':'application/wasm','.js':'text/javascript','.html':'text/html'};
const server=createServer((req,res)=>{
 const pathname=new URL(req.url,'http://localhost').pathname;
 const send=(type,body)=>{res.writeHead(200,{'content-type':type,...headers});res.end(body);};
 if(pathname==='/')return send('text/html','<!doctype html><title>GPU compute qualification</title>');
 if(pathname==='/guest.sit')return send('application/octet-stream',readFileSync(new URL('tests/toolbox-showcase/toolbox-showcase.sit',repo)));
 const local=resolve(dist,'.'+pathname);
 if(!local.startsWith(dist+'/') || !existsSync(local)){res.writeHead(404,headers);res.end();return;}
 send(types[extname(local)] || 'application/octet-stream',readFileSync(local));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const base=`http://127.0.0.1:${server.address().port}`;
const profile=await mkdtemp(join(tmpdir(),'systemless-gpu-compute-'));
const port=9400+Math.floor(Math.random()*1000);
const chromePath=process.env.CHROME_BIN || ['/Applications/Google Chrome.app/Contents/MacOS/Google Chrome','/usr/bin/google-chrome','/usr/bin/chromium'].find(existsSync);
if(!chromePath){await new Promise(resolve=>server.close(resolve));await rm(profile,{recursive:true,force:true});throw new Error('Set CHROME_BIN');}
const chrome=spawn(chromePath,['--headless=new',`--remote-debugging-port=${port}`,`--user-data-dir=${profile}`,'--no-first-run','--no-default-browser-check','about:blank'],{stdio:'ignore'});
let page,spawnError;
chrome.on('error',error=>{spawnError=error;});
try {
 const version=await waitForChrome(port);
 const browser=connect(version.webSocketDebuggerUrl);await browser.ready;
 const {targetId}=await browser.send('Target.createTarget',{url:'about:blank'});browser.close();
 const targets=await fetchJson(`http://127.0.0.1:${port}/json/list`);
 page=connect(targets.find(t=>t.id===targetId).webSocketDebuggerUrl);await page.ready;
 // Progress and runtime warnings (including CPU fallbacks) go to stderr.
 page.on('Runtime.consoleAPICalled',({type,args})=>console.error(`[page ${type}]`,args.map(arg=>arg.value ?? arg.description).join(' ')));
 await page.send('Page.enable');await page.send('Runtime.enable');
 await page.send('Page.navigate',{url:base});
 await new Promise(r=>setTimeout(r,500));
 const report=await evaluate(page,`(${qualify.toString()})(${JSON.stringify(runtime)},${JSON.stringify(wasm)},${JSON.stringify(cases)})`,900000);
 console.log(JSON.stringify({...report,browser:version.Browser},null,2));
} finally {
 page?.close();chrome.kill('SIGTERM');await new Promise(resolve=>server.close(resolve));
 await rm(profile,{recursive:true,force:true,maxRetries:20,retryDelay:250});
}

async function qualify(runtime,wasm,cases) {
 if(!crossOriginIsolated)throw new Error('Production headers did not isolate the page');
 if(!navigator.gpu)throw new Error('WebGPU unavailable');
 const moduleUrl=new URL(runtime,location.href).href, wasmUrl=new URL(wasm,location.href).href;
 const guestBytes=await (await fetch('/guest.sit')).arrayBuffer();

 async function play(architecture,actions,gpuCompute) {
  const worker=new Worker('/emulator-worker.js?runtime=gpu-compute-probe');
  const generation=1, inbox=[];
  let wake=null, last=null;
  worker.onmessage=({data})=>{inbox.push(data);wake?.();};
  worker.onerror=event=>{inbox.push({type:'error',message:event.message});wake?.();};
  const next=async(predicate,timeout)=>{
   const deadline=performance.now()+timeout;
   for(;;){
    const failure=inbox.find(m=>m.type==='error');
    if(failure)throw new Error('Runtime worker: '+failure.message);
    const index=inbox.findIndex(predicate);
    if(index>=0)return inbox.splice(index,1)[0];
    if(performance.now()>deadline)throw new Error('Timed out in '+architecture+' '+JSON.stringify(gpuCompute));
    await new Promise(r=>{wake=r;setTimeout(r,50);});
   }
  };
  const post=(type,fields={},transfer=[])=>worker.postMessage({type,generation,...fields},transfer);
  // Scenarios use the desktop harness's frontend clock: one tick per 60 Hz
  // host frame. Guest time can freeze (menu tracking), so never wait on it.
  let hostTicks=0;
  const frame=async()=>{
   await new Promise(r=>setTimeout(r,1000/60));
   post('frame',{queuedAudioSamples:0,forceRender:true,outputScale:1,cursorScale:0});
   last=await next(m=>m.type==='frame',120000);
   hostTicks++;
   if(!last.frame)throw new Error('Frame without RGBA image');
   return last;
  };
  const pixel=(x,y)=>{const i=(y*last.width+x)*4;return [last.frame[i],last.frame[i+1],last.frame[i+2]];};
  const near=(a,b,t=0)=>a.every((v,i)=>Math.abs(v-b[i])<=t);
  const run=async ticks=>{const end=hostTicks+ticks;while(hostTicks<end)await frame();};
  const label=`${architecture} ${gpuCompute ? JSON.stringify(gpuCompute) : 'cpu'}`;
  const started=performance.now();
  console.log('start',label);
  try {
   const bytes=guestBytes.slice(0);
   const config={id:'gpu-compute-'+architecture,architecture,launch_modifiers:[],show_menu_bar:true,screen_depth:null,application_partition_size:null,remove_paths:[],runtime_pacing:{max_ticks_per_paint:2,reset_slack_ticks:4,cpu_mhz:25,adaptive_min_cpu_mhz:null},arrows_as_numpad:false};
   post('boot',{protocolVersion:8,moduleUrl,wasmUrl,gameBytes:bytes,config:JSON.stringify(config),pluginForks:[],gpuCompute},[bytes]);
   await next(m=>m.type==='ready',180000);
   console.log('booted',label);
   let status=null;
   if(gpuCompute){
    status=await next(m=>m.type==='gpuCompute',60000);
    if(status.state!=='ready')throw new Error('GPU compute unavailable: '+JSON.stringify(status));
   }
   await frame();
   console.log('first frame',label,last.width+'x'+last.height,'tick',last.guestTick);
   const captures={};
   for(const action of actions){
    if(action.type==='run_until_pixel'){
     const end=hostTicks+action.timeout_ticks;
     while(!near(pixel(action.x,action.y),action.rgb,action.tolerance||0)){
      if(hostTicks>end)throw new Error(`${architecture}: pixel ${action.x},${action.y} stayed ${pixel(action.x,action.y)}`);
      await frame();
     }
    } else if(action.type==='run')await run(action.ticks);
    else if(action.type==='mouse_down')post('mouseDown',{v:action.v,h:action.h});
    else if(action.type==='mouse_move')post('mouseMove',{v:action.v,h:action.h});
    else if(action.type==='mouse_up')post('mouseUp',{v:action.v,h:action.h});
    else if(action.type==='screenshot')captures[action.name]={width:last.width,height:last.height,pixels:last.frame.slice()};
    else if(action.type==='assert_pixel'){
     if(!near(pixel(action.x,action.y),action.rgb,action.tolerance||0))throw new Error(`${architecture}${gpuCompute?' GPU':''}: pixel ${action.x},${action.y} is ${pixel(action.x,action.y)}, expected ${action.rgb}`);
    } else throw new Error('Unsupported action '+action.type);
   }
   // Let a pending injected loss land before reading final counters.
   if(gpuCompute?.injectFailureAfterMs>=0)await new Promise(r=>setTimeout(r,gpuCompute.injectFailureAfterMs+100)),await run(2);
   post('shutdown');
   await next(m=>m.type==='stopped',30000);
   console.log('done',label,Math.round(performance.now()-started)+'ms','tick',last.guestTick,JSON.stringify(last.gpuCompute||null));
   const timing=last.gpuComputeTiming;
   const latency=timing?.calls?{requests:timing.calls,meanMs:+(timing.totalMs/timing.calls).toFixed(3),maxMs:+timing.maxMs.toFixed(3)}:null;
   return {captures,status,stats:last.gpuCompute,latency,guestTick:last.guestTick};
  } finally {worker.terminate();}
 }

 const compare=(cpu,gpu,tolerance)=>{
  if(cpu.width!==gpu.width || cpu.height!==gpu.height)throw new Error('Capture sizes differ');
  let differing=0;
  for(let i=0;i<cpu.pixels.length;i+=4)if(!(Math.abs(cpu.pixels[i]-gpu.pixels[i])<=tolerance&&Math.abs(cpu.pixels[i+1]-gpu.pixels[i+1])<=tolerance&&Math.abs(cpu.pixels[i+2]-gpu.pixels[i+2])<=tolerance))differing++;
  return differing;
 };
 const results=[];
 let adapter=null;
 for(const test of cases){
  const cpu=await play(test.architecture,test.actions,undefined);
  const gpu=await play(test.architecture,test.actions,{mode:'force',timeoutMs:20000});
  adapter=gpu.status.adapter;
  if(!gpu.stats)throw new Error(test.name+': no GPU compute statistics');
  if(gpu.stats.disabled || ['raster','blit','qd3d'].some(kind=>gpu.stats[kind][1]))throw new Error(test.name+': GPU compute failures '+JSON.stringify(gpu.stats));
  const missing=test.kinds.filter(kind=>!gpu.stats[kind][0]);
  if(missing.length)throw new Error(test.name+': no completed GPU '+missing+' requests '+JSON.stringify(gpu.stats));
  const checkpoints=Object.keys(cpu.captures).map(name=>({name,differingPixels:compare(cpu.captures[name],gpu.captures[name],test.tolerance)}));
  if(!checkpoints.length || checkpoints.some(point=>point.differingPixels))throw new Error(test.name+': GPU captures differ '+JSON.stringify(checkpoints));
  results.push({name:test.name,architecture:test.architecture,tolerance:test.tolerance,checkpoints,gpuRequests:gpu.stats,latency:gpu.latency});
 }
 // Adaptive dispatch is the production default; it must run cleanly too.
 const adaptiveCase=cases.find(test=>test.name==='native-3d');
 const adaptive=await play(adaptiveCase.architecture,adaptiveCase.actions,{mode:'adaptive'});
 if(adaptive.stats.disabled || ['raster','blit','qd3d'].some(kind=>adaptive.stats[kind][1]))throw new Error('Adaptive GPU compute failed '+JSON.stringify(adaptive.stats));
 // Device loss while the 68k guest is drawing: work after the loss returns to
 // the CPU and the remaining captures still match.
 const lossCase=cases[0];
 const cpu=await play(lossCase.architecture,lossCase.actions,undefined);
 const lost=await play(lossCase.architecture,lossCase.actions,{mode:'force',timeoutMs:20000,injectFailureAfterMs:300});
 if(!/injected WebGPU device loss/.test(lost.stats.disabled||''))throw new Error('Device loss was not observed '+JSON.stringify(lost.stats));
 const lossCheckpoints=Object.keys(cpu.captures).map(name=>({name,differingPixels:compare(cpu.captures[name],lost.captures[name],0)}));
 if(lossCheckpoints.some(point=>point.differingPixels))throw new Error('Device-loss fallback differs '+JSON.stringify(lossCheckpoints));
 return {crossOriginIsolated,adapter,cases:results,adaptive:{name:adaptiveCase.name,gpuRequests:adaptive.stats},deviceLoss:{name:lossCase.name,checkpoints:lossCheckpoints,gpuRequests:lost.stats}};
}

async function evaluate(page,expression,timeout) {
 const result=await page.send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true,timeout});
 if(result.exceptionDetails)throw new Error(JSON.stringify(result.exceptionDetails));
 return result.result.value;
}

async function waitForChrome(port) {
 const deadline=Date.now()+15_000;
 let lastError=null;
 while(Date.now()<deadline){
  if(spawnError)throw spawnError;
  try {return await fetchJson(`http://127.0.0.1:${port}/json/version`);}
  catch(error){lastError=error;await new Promise(r=>setTimeout(r,100));}
 }
 throw lastError ?? new Error('Chrome did not start');
}

async function fetchJson(url) {
 const response=await fetch(url);
 if(!response.ok)throw new Error(`${url} -> HTTP ${response.status}`);
 return response.json();
}
