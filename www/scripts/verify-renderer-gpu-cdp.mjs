#!/usr/bin/env node
// Exact indexed/RGBA GPU differential test in a real OffscreenCanvas worker.
// Set CHROME_BIN for a non-macOS Chrome/Chromium installation.
import {spawn} from 'node:child_process';
import {createReadStream,existsSync} from 'node:fs';
import {mkdtemp,rm} from 'node:fs/promises';
import {createServer} from 'node:http';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
const modules = new Set(['/renderer-gpu.js', '/renderer-worker.js', '/renderer-transport.js']);
const profile=await mkdtemp(join(tmpdir(),'systemless-renderer-gpu-'));
const server=createServer((req,res)=>{
 if(modules.has(req.url)){
  res.writeHead(200,{'content-type':'text/javascript'});createReadStream(new URL('../src' + req.url, import.meta.url)).pipe(res);
 }else{res.writeHead(200,{'content-type':'text/html'});res.end('<!doctype html><title>Renderer differential probe</title>');}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const base=`http://127.0.0.1:${server.address().port}`;
const port=9400+Math.floor(Math.random()*1000);
const chromePath=process.env.CHROME_BIN || ['/Applications/Google Chrome.app/Contents/MacOS/Google Chrome','/usr/bin/google-chrome','/usr/bin/chromium','/usr/bin/chromium-browser'].find(existsSync);
if(!chromePath) { await new Promise(resolve=>server.close(resolve)); await rm(profile,{recursive:true,force:true}); throw new Error('Set CHROME_BIN to Chrome/Chromium'); }
const chrome=spawn(chromePath,['--headless=new',`--remote-debugging-port=${port}`,`--user-data-dir=${profile}`,'--no-first-run','--no-default-browser-check','about:blank'],{stdio:'ignore'});
let page,spawnError;
chrome.on('error',error=>{spawnError=error;});
try {
 const version=await waitForChrome(port);
 const browser=connect(version.webSocketDebuggerUrl);await browser.ready;
 const {targetId}=await browser.send('Target.createTarget',{url:'about:blank'});browser.close();
 const targets=await fetchJson(`http://127.0.0.1:${port}/json/list`);page=connect(targets.find(t=>t.id===targetId).webSocketDebuggerUrl);await page.ready;
 await page.send('Page.enable');await page.send('Runtime.enable');
 await page.send('Page.navigate',{url:base});
 const report=await evaluate(page,`(${exercise.toString()})()`,120000);
 report.browser=version.Browser; report.host={platform:process.platform,arch:process.arch};
 console.log(JSON.stringify(report,null,2));
} finally {
 if(page)page.close();
 chrome.kill('SIGTERM');await new Promise(resolve=>server.close(resolve));
 await rm(profile,{recursive:true,force:true,maxRetries:20,retryDelay:250});
}
async function exercise(){
 const task = async base => {
  const { GpuFramePresenter } = await import(base + '/renderer-gpu.js');
  const canvas = new OffscreenCanvas(1,1), presenter = new GpuFramePresenter(canvas), gl=presenter.gl;
  let cases=0,pixels=0;
  for(const width of [1,3,257]) for(const height of [1,3,17]) for(const padding of [0,7]) {
   const stride=width+padding, indices=new Uint8Array(new ArrayBuffer(stride*height+13),13,stride*height), palette=new Uint8Array(new ArrayBuffer(1031),7,1024);
   for(let i=0;i<indices.length;i++) indices[i]=(i*71+13)&255;
   for(let i=0;i<256;i++) palette.set([i,(i*37)&255,255-i,255],i*4);
   for(const shift of [0,97]) {
    for(let i=0;i<256;i++) palette[i*4]=(i+shift)&255;
    const expected=new Uint8Array(new ArrayBuffer(width*height*4+9),9,width*height*4);
    for(let y=0;y<height;y++) for(let x=0;x<width;x++) expected.set(palette.subarray(indices[y*stride+x]*4,indices[y*stride+x]*4+4),(y*width+x)*4);
    for(const kind of ['indexed8','rgba']) {
     presenter.paint({kind,complete:true,width,height,stride,pixels:kind==='rgba'?expected:indices,palette});
     const actual=new Uint8Array(expected.length);gl.readPixels(0,0,width,height,gl.RGBA,gl.UNSIGNED_BYTE,actual);
     for(let y=0;y<height;y++)for(let x=0;x<width*4;x++) {
      const a=actual[(height-1-y)*width*4+x],e=expected[y*width*4+x];
      if(a!==e)throw new Error(JSON.stringify({width,height,padding,shift,kind,y,x,actual:a,expected:e}));
     }
     const error=gl.getError();if(error!==gl.NO_ERROR)throw new Error('GL error '+error);
     cases++;pixels+=width*height;
    }
   }
  }
  const driver=gl.getExtension('WEBGL_debug_renderer_info');
  const result={cases,pixels,maxTextureSize:presenter.maxTextureSize,renderer:gl.getParameter(gl.RENDERER),
   driver:driver?gl.getParameter(driver.UNMASKED_RENDERER_WEBGL):null,exact:true};
  presenter.dispose();
  const { RendererTransport } = await import(base + '/renderer-transport.js');
  const endpoint = new Worker(base + '/renderer-worker.js');
  const identity = { generation: 9, rendererGeneration: 3, protocolVersion: 1 };
  try {
   const ready = new Promise((resolve,reject) => {
    endpoint.onmessage = ({data}) => data.type==='ready'?resolve(data):reject(new Error(JSON.stringify(data)));
    endpoint.onerror = event => reject(new Error(event.message));
   });
   const screen = new OffscreenCanvas(4,4);
   endpoint.postMessage({...identity,type:'init',backend:'webgl',canvas:screen},[screen]);
   const capability = await ready;
   if(capability.backend!=='offscreen-webgl'||!capability.kinds.includes('indexed8'))throw new Error('GPU capability missing');
   let submitted=0;
   let transport;
   await new Promise((resolve,reject) => {
    transport = new RendererTransport(endpoint,identity,{onFailure:reject,onSubmitted:metrics=>{
     submitted++;if(metrics.sequence===100)resolve();
    }});
    endpoint.onmessage = ({data}) => transport.receive(data);
    endpoint.onerror = event => reject(new Error(event.message));
    for(let sequence=1;sequence<=100;sequence++) {
     const palette = new Uint8Array(1024);for(let i=0;i<256;i++)palette.set([i,255-i,sequence,255],i*4);
     transport.submit({kind:'indexed8',complete:true,sequence,displayGeneration:1,width:4,height:4,stride:7,
      pixels:new Uint8Array(28).fill(sequence),palette});
    }
   });
   if(submitted!==2||transport.inFlight||transport.pending||transport.recycled.length>2)throw new Error('GPU transport is not bounded');
   result.workerTransport={submitted,lastSequence:100,recycled:transport.recycled.length};
   transport.dispose();
   const stopped=new Promise((resolve,reject)=>{endpoint.onmessage=({data})=>data.type==='stopped'?resolve():reject(new Error(JSON.stringify(data)));});
   endpoint.postMessage({...identity,type:'stop'});await stopped;
  } finally { endpoint.terminate(); }
  return result;
 };
 const blob=new Blob([`(${task.toString()})(${JSON.stringify(location.origin)}).then(result=>postMessage({result})).catch(error=>postMessage({error:String(error.stack)}));`],{type:'text/javascript'});
 const url=URL.createObjectURL(blob),worker=new Worker(url,{type:'module'});
 try{return await new Promise((resolve,reject)=>{worker.onmessage=({data})=>data.error?reject(new Error(data.error)):resolve(data.result);worker.onerror=event=>reject(new Error(event.message));});}
 finally{worker.terminate();URL.revokeObjectURL(url);}
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

function connect(webSocketUrl) {
  const ws = new WebSocket(webSocketUrl);
  let nextId = 1;
  const pending = new Map();
  const handlers = new Map();
  const ready = new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve, { once: true });
    ws.addEventListener("error", reject, { once: true });
  });

  ws.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const { resolve, reject } = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) {
        reject(new Error(JSON.stringify(message.error)));
      } else {
        resolve(message.result ?? {});
      }
      return;
    }

    const methodHandlers = handlers.get(message.method);
    if (methodHandlers) {
      for (const handler of methodHandlers) {
        handler(message.params ?? {});
      }
    }
  });

  return {
    ready,
    close: () => ws.close(),
    on(method, handler) {
      if (!handlers.has(method)) {
        handlers.set(method, []);
      }
      handlers.get(method).push(handler);
    },
    send(method, params = {}) {
      const id = nextId++;
      ws.send(JSON.stringify({ id, method, params }));
      return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
    },
  };
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
