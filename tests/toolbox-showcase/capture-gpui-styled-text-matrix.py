#!/usr/bin/env python3
"""Capture 192 styled field cases using the shared macOS GPUI compositor.

Build the gpui-menu-demo example with gpui-demo-test from the current clean
source first. Use a fresh output directory. This records guest/composed PNGs
and actual depth/state sidecars; archive and review results before claiming
qualification. It is not a native Macintosh oracle or production ownership test.
"""
import argparse,hashlib,importlib.util,json,pathlib,subprocess
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('output',type=pathlib.Path)
args=parser.parse_args()
root=pathlib.Path(__file__).resolve().parents[2];out=args.output.resolve()
if out.exists() and any(out.iterdir()):parser.error('use a fresh output directory; do not restart a live capture job')
out.mkdir(parents=True,exist_ok=True)
binary=root/'target/debug/examples/gpui-menu-demo'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
binary_hash=sha(binary)
s=importlib.util.spec_from_file_location('verify',root/'tests/toolbox-showcase/verify-gpui-styled-text-ink.py');v=importlib.util.module_from_spec(s);s.loader.exec_module(v)
manifest={'source_commit':source,'capture_binary_sha256':binary_hash,'fixture':'tests/toolbox-showcase/toolbox-showcase.sit','fixture_sha256':sha(root/'tests/toolbox-showcase/toolbox-showcase.sit'),'field_bounds':[126,74,164,561],'scope':'Public mixed-style field raster only; no production ownership, general themes/backgrounds, GPUI pointer mapping or native host observer qualification','cases':[],'complete':False}
configs=[('inactive','inactive',None,'--capture-styled-text-edit-ink'),*[( 'selection',state,None,flag) for state,flag in [('selected','--capture-styled-text-edit-selected'),('suspended','--capture-styled-text-edit-selected-suspended'),('resumed','--capture-styled-text-edit-selected-resumed')]],*[( 'caret',state,offset,'--capture-styled-text-edit-caret') for offset in [0,26] for state in ['visible','blink-off','suspended','resumed']]]
for mode,depth,flags in [('ppc16',16,['--prefer-powerpc']),('ppc8',8,['--prefer-powerpc','--screen-depth','8']),('mono',1,['--screen-depth','1']),('colour',8,['--screen-depth','8'])]:
 for kind,state,offset,flag in configs:
  for scale in [0.75,1,1.5,2]:
   assert sha(binary)==binary_hash,'capture executable changed during qualification'
   name=f'{kind}-{mode}-{state}-{offset if offset is not None else "none"}-{scale}'
   path=out/(name+'.png');cmd=[str(binary),str(root/manifest['fixture']),*flags,'--capture-scale',str(scale),flag,str(path)]
   if kind=='caret':cmd+=['--capture-styled-caret-offset',str(offset),'--capture-styled-caret-state',state]
   result=subprocess.run(cmd,capture_output=True,text=True)
   if result.returncode:
    (out/(name+'.failure.txt')).write_text(result.stdout+result.stderr)
    print('FAILED '+name+'\n'+result.stderr,flush=True);raise SystemExit(result.returncode)
   evidence=json.loads(path.with_suffix('.json').read_text())
   assert evidence['depth']==depth,(name,evidence)
   assert evidence['drawing_intact'] and evidence['active']==(state not in ['inactive','suspended']),(name,evidence)
   expected=[0,26] if kind=='selection' else [offset or 0,offset or 0]
   assert evidence['selection']==expected,(name,evidence)
   if kind=='caret':
    assert evidence['caret_state']==state and evidence['insertion_offset']==offset
    if state!='suspended':assert evidence['caret_visible']==(state!='blink-off'),(name,evidence)
   case={'kind':kind,'mode':mode,'actual_depth':depth,'state':state,'insertion_offset':offset,'scale':scale,'device_scale':2,'rendered':path.name,'guest':path.with_suffix('.guest.png').name,'evidence':path.with_suffix('.json').name,'command':cmd,'rendered_sha256':sha(path),'guest_sha256':sha(path.with_suffix('.guest.png')),'evidence_sha256':sha(path.with_suffix('.json'))}
   v.verify_case(out,case,manifest['field_bounds'])
   manifest['cases'].append(case)
   (out/'progress.json').write_text(json.dumps(manifest,indent=2)+'\n')
   print(f'{len(manifest["cases"])}/192 PASS {name} depth={depth}',flush=True)
assert len(manifest['cases'])==192
manifest['complete']=True;(out/'progress.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('All 192 corrected styled field captures passed',flush=True)
