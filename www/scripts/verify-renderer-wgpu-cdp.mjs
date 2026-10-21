#!/usr/bin/env node
// Build with `cd www && trunk build`, then pass the dist directory as argv[2].
// Requires real browser WebGPU; unavailable adapters fail rather than skip.
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
const profile=await mkdtemp(join(tmpdir(),'systemless-wgpu-'));
const server=createServer((req,res)=>{
 const pathname=new URL(req.url,'http://localhost').pathname;
 if(pathname==='/') {res.writeHead(200,{'content-type':'text/html'});res.end('<!doctype html><title>wgpu qualification</title>');return;}
 if(pathname==='/guest.sit'){res.writeHead(200,{'content-type':'application/octet-stream'});res.end(readFileSync(new URL('../../tests/toolbox-showcase/toolbox-showcase.sit',import.meta.url)));return;}
 const local=resolve(dist,'.'+pathname);
 if(!local.startsWith(dist+'/') || !existsSync(local)){res.writeHead(404);res.end();return;}
 res.writeHead(200,{'content-type':extname(local)==='.wasm'?'application/wasm':'text/javascript'});
 let content=readFileSync(local);
 if(pathname==='/renderer-worker.js') content=Buffer.concat([content,Buffer.from(`
 const productionMessage=self.onmessage;
 self.onmessage=event=>{if(event.data?.type==='probeLoseDevice'){wgpu.dispose();return;}return productionMessage(event);};
 `)]);
 res.end(content);
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const base=`http://127.0.0.1:${server.address().port}`;
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
 await page.send('Page.enable');await page.send('Runtime.enable');
 await page.send('Page.navigate',{url:base});
 const report=await evaluate(page,`(${exercise.toString()})(${JSON.stringify(runtime)},${JSON.stringify(wasm)})`,120000);
 console.log(JSON.stringify({...report,browser:version.Browser},null,2));
} finally {
 page?.close();chrome.kill('SIGTERM');await new Promise(resolve=>server.close(resolve));
 await rm(profile,{recursive:true,force:true,maxRetries:20,retryDelay:250});
}
async function exercise(runtime,wasm) {
 async function guestFrame(moduleUrl,wasmUrl,origin,architecture) {
   const bindings=await import(moduleUrl);await bindings.default({module_or_path:wasmUrl});
   const bytes=new Uint8Array(await (await fetch(origin+'/guest.sit')).arrayBuffer());
   const config={id:'wgpu-probe-'+architecture,architecture,launch_modifiers:[],show_menu_bar:true,screen_depth:8,application_partition_size:null,remove_paths:[],runtime_pacing:{max_ticks_per_paint:2,reset_slack_ticks:4,cpu_mhz:25,adaptive_min_cpu_mhz:null},arrows_as_numpad:false};
   const machine=await bindings.WorkerMachine.create(bytes,JSON.stringify(config),[],()=>{});
   try {
     let frame;
     for(let i=0;i<60;i++){frame=machine.runFrame(0,false,1,true,false,false,false,1);await new Promise(r=>setTimeout(r,16));}
     return {frame:frame.frame,width:frame.width,height:frame.height,guestTick:frame.guestTick,running:frame.running};
   } finally {machine.free();}
 }

 if(!navigator.gpu)throw new Error('WebGPU unavailable');
 const {RendererClient}=await import('/renderer-client.js');
 const parent=document.createElement('div');parent.style.cssText='position:relative;width:65px;height:37px';
 const canvas=document.createElement('canvas');canvas.width=65;canvas.height=37;parent.append(canvas);document.body.append(parent);
 const options={backend:'wgpu',moduleUrl:new URL(runtime,location.href).href,wasmUrl:new URL(wasm,location.href).href};
 const client=new RendererClient(canvas,'/renderer-worker.js',1,options);
 const wait=async predicate=>{const deadline=performance.now()+30000;while(!predicate()){if(performance.now()>deadline)throw new Error('Timed out: '+JSON.stringify(client.status()));await new Promise(r=>setTimeout(r,10));}};
 const results=[];
 try {
  await wait(()=>client.status().phase==='ready' || client.status().phase==='failed');
  if(client.status().backend!=='offscreen-wgpu')throw new Error('wgpu required: '+JSON.stringify(client.status()));
  for(const [width,height] of [[65,37],[3,2],[127,59],[65,37]]) {
   const pixels=new Uint8Array(width*height*4);
   for(let y=0;y<height;y++)for(let x=0;x<width;x++) {const i=(y*width+x)*4;pixels.set([(x*17+y*3)%256,(y*29+x*7)%256,(x^y)*11%256,255],i);}
   client.paint(width,height,pixels.slice(),'none');
   await wait(()=>(client.status().submitted && !client.status().pending) || client.status().phase==='failed');
   if(client.status().phase==='failed')throw new Error(JSON.stringify(client.status()));
   await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
   const read=document.createElement('canvas');read.width=width;read.height=height;
   const context=read.getContext('2d');context.drawImage(client.canvas,0,0);
   const actual=context.getImageData(0,0,width,height).data;
   if(!pixels.length || actual.length!==pixels.length)throw new Error('Invalid reference image size');
   let different=0;for(let i=0;i<pixels.length;i++)if(actual[i]!==pixels[i])different++;
   results.push({width,height,differentBytes:different});
   if(different)throw new Error('wgpu pixel mismatch: '+JSON.stringify(results.at(-1))+' first='+Array.from(actual.slice(0,24)));
  }
  const guests=[];
  for(const architecture of ['68k','ppc']) {
    const source=`(${guestFrame.toString()})(${JSON.stringify(options.moduleUrl)},${JSON.stringify(options.wasmUrl)},${JSON.stringify(location.origin)},${JSON.stringify(architecture)}).then(frame=>postMessage({frame},[frame.frame.buffer])).catch(error=>postMessage({error:String(error.stack || error)}));`;
    const url=URL.createObjectURL(new Blob([source],{type:'text/javascript'}));
    const worker=new Worker(url,{type:'module'});
    let frame;
    try {
      frame=await new Promise((resolve,reject)=>{
        const timer=setTimeout(()=>reject(new Error('Guest boot timed out: '+architecture)),60000);
        worker.onmessage=({data})=>{clearTimeout(timer);data.error?reject(new Error(data.error)):resolve(data.frame);};
        worker.onerror=event=>{clearTimeout(timer);reject(new Error(event.message));};
      });
    } finally {worker.terminate();URL.revokeObjectURL(url);}
    if(!frame.running || frame.guestTick<=0 || !frame.frame?.length)throw new Error('Guest failed: '+architecture);
    const colors=new Set();for(let i=0;i<frame.frame.length;i+=4)colors.add(frame.frame[i]+frame.frame[i+1]*256+frame.frame[i+2]*65536);
    if(colors.size<3)throw new Error('Guest framebuffer is blank: '+architecture);
    client.paint(frame.width,frame.height,frame.frame.slice(),'none');
    await wait(()=>(client.status().submitted && !client.status().pending) || client.status().phase==='failed');
    if(client.status().phase==='failed')throw new Error(JSON.stringify(client.status()));
    await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
    const read=document.createElement('canvas');read.width=frame.width;read.height=frame.height;
    const ctx=read.getContext('2d');ctx.drawImage(client.canvas,0,0);
    const actual=ctx.getImageData(0,0,frame.width,frame.height).data;
    if(!frame.frame.length || actual.length!==frame.frame.length)throw new Error('Invalid guest reference image size');
    let different=0;for(let i=0;i<actual.length;i++)if(actual[i]!==frame.frame[i])different++;
    if(different)throw new Error(architecture+' guest pixel mismatch: '+different+' actual='+Array.from(actual.slice(0,16))+' expected='+Array.from(frame.frame.slice(0,16))+' size='+frame.width+'x'+frame.height+' canvas='+client.canvas.width+'x'+client.canvas.height);
    guests.push({architecture,guestTick:frame.guestTick,colors:colors.size,differentBytes:different});
  }
  client.worker.postMessage({type:'probeLoseDevice'});
  await wait(()=>client.status().phase==='failed');
  const failure=client.status().error;
  if(!/device lost/i.test(failure))throw new Error('Missing device loss: '+failure);
  return {backend:'offscreen-wgpu',exactPixels:true,resizeCases:results,deviceLossObserved:true,guests};
 } finally {client.dispose();parent.remove();}
}

async function evaluate(page, expression, timeout) {
  const result = await page.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
    timeout,
  });
  if (result.exceptionDetails) {
    throw new Error(JSON.stringify(result.exceptionDetails));
  }
  return result.result.value;
}

async function waitForChrome(port) {
  const deadline = Date.now() + 15_000;
  let lastError = null;
  while (Date.now() < deadline) {
    if(spawnError)throw spawnError;
    try {
      return await fetchJson(`http://127.0.0.1:${port}/json/version`);
    } catch (error) {
      lastError = error;
      await sleep(100);
    }
  }
  throw lastError ?? new Error("Chrome did not start");
}

async function fetchJson(url) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`${url} -> HTTP ${response.status}`);
  }
  return response.json();
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
