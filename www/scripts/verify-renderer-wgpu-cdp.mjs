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
 if(pathname==='/compact-native.json'){res.writeHead(200,{'content-type':'application/json'});res.end(readFileSync(new URL('../tests/fixtures/compact-native.json',import.meta.url)));return;}
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
 const report=await evaluate(page,`(${exercise.toString()})(${JSON.stringify(runtime)},${JSON.stringify(wasm)})`,180000);
 console.log(JSON.stringify({...report,browser:version.Browser},null,2));
} finally {
 page?.close();chrome.kill('SIGTERM');await new Promise(resolve=>server.close(resolve));
 await rm(profile,{recursive:true,force:true,maxRetries:20,retryDelay:250});
}
async function exercise(runtime,wasm) {
 async function guestFrames(moduleUrl,wasmUrl,origin,architecture,outputScale) {
   const bindings=await import(moduleUrl);await bindings.default({module_or_path:wasmUrl});
   const bytes=new Uint8Array(await (await fetch(origin+'/guest.sit')).arrayBuffer());
   const config={id:'wgpu-probe-'+architecture+'-'+outputScale,architecture,launch_modifiers:[],show_menu_bar:true,screen_depth:8,application_partition_size:null,remove_paths:[],runtime_pacing:{max_ticks_per_paint:2,reset_slack_ticks:4,cpu_mhz:25,adaptive_min_cpu_mhz:null},arrows_as_numpad:false};
   const machine=await bindings.WorkerMachine.create(bytes,JSON.stringify(config),[],()=>{});
   try {
     if(outputScale===1)machine.mouseMove(12,18);
     let frame;
     for(let i=0;i<60;i++){frame=machine.runFrame(0,false,outputScale,true,true,true,false,1);await new Promise(r=>setTimeout(r,16));}
     return {
       frame:frame.frame,
       indexedFrame:frame.indexedFrame,
       compactFrame:frame.compactFrame,
       width:frame.width,
       height:frame.height,
       guestTick:frame.guestTick,
       running:frame.running
     };
   } finally {machine.free();}
 }

 if(!navigator.gpu)throw new Error('WebGPU unavailable');
 const {RendererClient}=await import('/renderer-client.js');
 const {GpuFramePresenter,compactBytes}=await import('/renderer-gpu.js');
 const options={backend:'wgpu',moduleUrl:new URL(runtime,location.href).href,wasmUrl:new URL(wasm,location.href).href};

 // 1. Exhaustive direct WebGpuPresenter differential suite (RGBA, indexed8, cursor, compact).
 const bindings=await import(options.moduleUrl);
 await bindings.default({module_or_path:options.wasmUrl});
 const directScreen=new OffscreenCanvas(1,1);
 const directRead=new OffscreenCanvas(1,1);
 const directCtx=directRead.getContext('2d',{willReadFrequently:true});
 const presenter=await bindings.WebGpuPresenter.create(directScreen);
 let directCases=0,directCompactCases=0;
 try {
   const readDirect=(w,h)=>{
     directRead.width=w;directRead.height=h;
     directCtx.drawImage(directScreen,0,0);
     return directCtx.getImageData(0,0,w,h).data;
   };
   for(const width of [1,3,257]) for(const height of [1,3,17]) for(const padding of [0,7]) {
     const stride=width+padding;
     const indices=new Uint8Array(new ArrayBuffer(stride*height+13),13,stride*height);
     const palette=new Uint8Array(new ArrayBuffer(1031),7,1024);
     for(let i=0;i<indices.length;i++)indices[i]=(i*71+13)&255;
     for(let i=0;i<256;i++)palette.set([i,(i*37)&255,255-i,255],i*4);
     for(const shift of [0,97]) {
       for(let i=0;i<256;i++)palette[i*4]=(i+shift)&255;
       const expected=new Uint8Array(width*height*4);
       for(let y=0;y<height;y++)for(let x=0;x<width;x++)expected.set(palette.subarray(indices[y*stride+x]*4,indices[y*stride+x]*4+4),(y*width+x)*4);
       const cursors=[
         undefined,
         {x:width-1,y:height-1,width:1,height:1,pixels:new Uint8Array([17,203,91,255])},
         {x:0,y:0,width:Math.min(3,width),height:Math.min(2,height),pixels:new Uint8Array(Math.min(3,width)*Math.min(2,height)*4).fill(255)},
       ];
       // Cursor patches are opaque (the runtime writes alpha 255) and the canvas is
       // opaque, so synthetic patches keep alpha at 255.
       if(width>=65)cursors.push({x:2,y:0,width:65,height:1,pixels:new Uint8Array(65*4).map((_,i)=>(i&3)===3?255:i&255)});
       for(const kind of ['indexed8','rgba']) for(const cursor of cursors) {
         const comparison=expected.slice();
         if(cursor)for(let y=0;y<cursor.height;y++)comparison.set(cursor.pixels.subarray(y*cursor.width*4,(y+1)*cursor.width*4),((cursor.y+y)*width+cursor.x)*4);
         directScreen.width=width;directScreen.height=height;
         if(kind==='indexed8'){
           presenter.paint_indexed8(width,height,stride,indices,palette,
             cursor?cursor.x:0,cursor?cursor.y:0,cursor?cursor.width:0,cursor?cursor.height:0,cursor?cursor.pixels:new Uint8Array(0));
         } else if(cursor) {
           presenter.paint_rgba_cursor(width,height,expected,cursor.x,cursor.y,cursor.width,cursor.height,cursor.pixels);
         } else {
           presenter.paint(width,height,expected);
         }
         await presenter.completed();
         const actual=readDirect(width,height);
         for(let i=0;i<comparison.length;i++)if(actual[i]!==comparison[i])throw new Error(JSON.stringify({direct:true,width,height,padding,shift,kind,hasCursor:!!cursor,i,actual:actual[i],expected:comparison[i]}));
         directCases++;
       }
     }
   }
   const vectors=await (await fetch('/compact-native.json')).json();
   for(const source of vectors) for(const output of source.outputs) {
     const width=source.width*output.scale,height=source.height*output.scale;
     directScreen.width=width;directScreen.height=height;
     presenter.paint_compact(width,height,source.width,source.height,source.scale,
       compactBytes(new Uint32Array(source.cells)),compactBytes(new Uint32Array(source.detail)));
     await presenter.completed();
     const actual=readDirect(width,height);
     for(let p=0;p<width*height;p++) {
       const argb=output.argb[p],idx=p*4;
       const exp=[(argb>>>16)&255,(argb>>>8)&255,argb&255,255];
       for(let c=0;c<4;c++)if(actual[idx+c]!==exp[c])throw new Error(JSON.stringify({directCompact:true,scale:source.scale,output:output.scale,p,c,actual:actual[idx+c],expected:exp[c]}));
     }
     directCompactCases++;
   }
   // Exercise storage buffer growth (>256 bytes) on both detail and cells buffers, plus mode transitions.
   const bigOffset=70000,bigDetail=new Uint32Array(bigOffset+16);
   for(let i=0;i<16;i++)bigDetail[bigOffset+i]=((i*17)<<16)|((255-i*17)<<8)|i;
   directScreen.width=4;directScreen.height=4;
   presenter.paint_compact(4,4,1,1,4,compactBytes(new Uint32Array([0x80000000|bigOffset])),compactBytes(bigDetail));
   await presenter.completed();
   const bigActual=readDirect(4,4);
   for(let i=0;i<16;i++) {
     const word=bigDetail[bigOffset+i],idx=i*4;
     if(bigActual[idx]!==((word>>>16)&255)||bigActual[idx+1]!==((word>>>8)&255)||bigActual[idx+2]!==(word&255)||bigActual[idx+3]!==255)
       throw new Error('Large compact detail buffer growth mismatch at '+i);
   }
   const bigCells=new Uint32Array(17*9);
   for(let i=0;i<bigCells.length;i++)bigCells[i]=((i&255)<<16)|(((i*7)&255)<<8)|((255-(i&255))&255);
   directScreen.width=17;directScreen.height=9;
   presenter.paint_compact(17,9,17,9,2,compactBytes(bigCells),new Uint8Array(0));
   await presenter.completed();
   const cellsActual=readDirect(17,9);
   for(let i=0;i<bigCells.length;i++) {
     const word=bigCells[i],idx=i*4;
     if(cellsActual[idx]!==((word>>>16)&255)||cellsActual[idx+1]!==((word>>>8)&255)||cellsActual[idx+2]!==(word&255))
       throw new Error('Large compact cells buffer growth mismatch at '+i);
   }
   directCompactCases+=2;
 } finally {
   presenter.dispose();presenter.free();
 }

 // 2. Worker RendererClient qualification (offscreen-wgpu): capabilities, resize, packet kinds, live 68k/PPC guests, device loss.
 const parent=document.createElement('div');parent.style.cssText='position:relative;width:65px;height:37px';
 const canvas=document.createElement('canvas');canvas.width=65;canvas.height=37;parent.append(canvas);document.body.append(parent);
 const client=new RendererClient(canvas,'/renderer-worker.js',1,options);
 const wait=async predicate=>{const deadline=performance.now()+30000;while(!predicate()){if(performance.now()>deadline)throw new Error('Timed out: '+JSON.stringify(client.status()));await new Promise(r=>setTimeout(r,10));}};
 const submitAndRead=async (packet,width,height)=>{
   const prevSeq=Number(client.logicalCanvas.getAttribute('data-render-sequence')||'0');
   if(!client.paintPacket(packet))throw new Error('paintPacket rejected: '+JSON.stringify(client.status()));
   await wait(()=>((Number(client.logicalCanvas.getAttribute('data-render-sequence')||'0')>prevSeq) && !client.status().pending) || client.status().phase==='failed');
   if(client.status().phase==='failed')throw new Error(JSON.stringify(client.status()));
   await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
   const read=document.createElement('canvas');read.width=width;read.height=height;
   const ctx=read.getContext('2d');ctx.drawImage(client.canvas,0,0);
   return ctx.getImageData(0,0,width,height).data;
 };
 const results=[];
 try {
  await wait(()=>client.status().phase==='ready' || client.status().phase==='failed');
  if(client.status().backend!=='offscreen-wgpu')throw new Error('wgpu required: '+JSON.stringify(client.status()));
  if(JSON.stringify(client.status().kinds)!==JSON.stringify(['rgba','indexed8','compact']))
    throw new Error('Unexpected wgpu kinds: '+JSON.stringify(client.status().kinds));
  for(const [width,height] of [[65,37],[3,2],[127,59],[65,37]]) {
   const pixels=new Uint8Array(width*height*4);
   for(let y=0;y<height;y++)for(let x=0;x<width;x++) {const i=(y*width+x)*4;pixels.set([(x*17+y*3)%256,(y*29+x*7)%256,(x^y)*11%256,255],i);}
   const actual=await submitAndRead({kind:'rgba',complete:true,width,height,pixels:pixels.slice(),cursorCss:'none'},width,height);
   if(!pixels.length || actual.length!==pixels.length)throw new Error('Invalid reference image size');
   let different=0;for(let i=0;i<pixels.length;i++)if(actual[i]!==pixels[i])different++;
   results.push({width,height,differentBytes:different});
   if(different)throw new Error('wgpu pixel mismatch: '+JSON.stringify(results.at(-1))+' first='+Array.from(actual.slice(0,24)));
  }

  // Worker indexed8 path: padded stride, cursor patch, palette update, and cursor removal.
  const workerStride=70,workerW=65,workerH=37;
  const workerIndices=new Uint8Array(workerStride*workerH);
  for(let i=0;i<workerIndices.length;i++)workerIndices[i]=(i*43+9)&255;
  for(const [shift,cursor] of [
    [0,{x:62,y:35,width:3,height:2,pixels:new Uint8Array(3*2*4).map((_,i)=>(i&3)===3?255:(i*53)&255)}],
    [113,undefined],
  ]) {
    const palette=new Uint8Array(1024);
    for(let i=0;i<256;i++)palette.set([(i+shift)&255,(i*29)&255,255-i,255],i*4);
    const expected=new Uint8Array(workerW*workerH*4);
    for(let y=0;y<workerH;y++)for(let x=0;x<workerW;x++)expected.set(palette.subarray(workerIndices[y*workerStride+x]*4,workerIndices[y*workerStride+x]*4+4),(y*workerW+x)*4);
    if(cursor)for(let y=0;y<cursor.height;y++)expected.set(cursor.pixels.subarray(y*cursor.width*4,(y+1)*cursor.width*4),((cursor.y+y)*workerW+cursor.x)*4);
    const actual=await submitAndRead({
      kind:'indexed8',complete:true,width:workerW,height:workerH,stride:workerStride,
      pixels:workerIndices.slice(),palette,
      cursor:cursor?{...cursor,pixels:cursor.pixels.slice()}:undefined,
      cursorCss:'none'
    },workerW,workerH);
    for(let i=0;i<expected.length;i++)if(actual[i]!==expected[i])throw new Error(`worker indexed8 mismatch shift=${shift} i=${i}`);
  }

  // Reference WebGL presenter for exact cross-backend checks on live guest compact packets.
  const refCanvas=new OffscreenCanvas(1,1);
  const refPresenter=new GpuFramePresenter(refCanvas);
  const refGl=refPresenter.gl;
  const readWebgl=(packet,w,h)=>{
    refPresenter.paint(packet);
    const raw=new Uint8Array(w*h*4);
    refGl.readPixels(0,0,w,h,refGl.RGBA,refGl.UNSIGNED_BYTE,raw);
    const topDown=new Uint8Array(w*h*4);
    for(let y=0;y<h;y++)topDown.set(raw.subarray((h-1-y)*w*4,(h-y)*w*4),y*w*4);
    return topDown;
  };

  const guests=[];
  try {
    for(const [architecture,outputScale] of [['68k',1],['ppc',1],['68k',2],['ppc',2]]) {
      const transfers=`const transfers=[f.frame?.buffer,f.indexedFrame?.pixels?.buffer,f.indexedFrame?.palette?.buffer,f.indexedFrame?.cursor?.pixels?.buffer,f.compactFrame?.compact?.cells?.buffer,f.compactFrame?.compact?.detail?.buffer].filter(Boolean);postMessage({frame:f},[...new Set(transfers)]);`;
      const source=`(${guestFrames.toString()})(${JSON.stringify(options.moduleUrl)},${JSON.stringify(options.wasmUrl)},${JSON.stringify(location.origin)},${JSON.stringify(architecture)},${outputScale}).then(f=>{${transfers}}).catch(error=>postMessage({error:String(error.stack || error)}));`;
      const url=URL.createObjectURL(new Blob([source],{type:'text/javascript'}));
      const worker=new Worker(url,{type:'module'});
      let out;
      try {
        out=await new Promise((resolve,reject)=>{
          const timer=setTimeout(()=>reject(new Error('Guest boot timed out: '+architecture)),60000);
          worker.onmessage=({data})=>{clearTimeout(timer);data.error?reject(new Error(data.error)):resolve(data.frame);};
          worker.onerror=event=>{clearTimeout(timer);reject(new Error(event.message));};
        });
      } finally {worker.terminate();URL.revokeObjectURL(url);}
      if(!out.running || out.guestTick<=0)throw new Error('Guest failed: '+architecture);
      const packet=out.compactFrame || out.indexedFrame;
      if(!packet)throw new Error(`Expected compact or indexed8 packet for ${architecture}@${outputScale}x`);
      const expected=readWebgl({
        ...packet,
        pixels:packet.pixels?.slice(),
        palette:packet.palette?.slice(),
        cursor:packet.cursor?{...packet.cursor,pixels:packet.cursor.pixels.slice()}:undefined,
        compact:packet.compact?{...packet.compact,cells:packet.compact.cells.slice(),detail:packet.compact.detail.slice()}:undefined,
      },out.width,out.height);
      const colors=new Set();for(let i=0;i<expected.length;i+=4)colors.add(expected[i]+expected[i+1]*256+expected[i+2]*65536);
      if(colors.size<3)throw new Error('Guest framebuffer is blank: '+architecture);
      const actual=await submitAndRead(packet,out.width,out.height);
      let different=0;for(let i=0;i<actual.length;i++)if(actual[i]!==expected[i])different++;
      if(different)throw new Error(`${architecture}@${outputScale}x (${packet.kind}) pixel mismatch: ${different}`);
      guests.push({architecture,outputScale,kind:packet.kind,guestTick:out.guestTick,colors:colors.size,differentBytes:different});
    }
  } finally {
    refPresenter.dispose();
  }

  client.worker.postMessage({type:'probeLoseDevice'});
  await wait(()=>client.status().phase==='failed');
  const failure=client.status().error;
  if(!/device lost/i.test(failure))throw new Error('Missing device loss: '+failure);
  return {backend:'offscreen-wgpu',exactPixels:true,directCases,directCompactCases,resizeCases:results,deviceLossObserved:true,guests};
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
