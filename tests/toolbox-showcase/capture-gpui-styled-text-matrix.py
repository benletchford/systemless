#!/usr/bin/env python3
"""Capture 192 styled field cases using the shared macOS GPUI compositor.

Build the gpui-menu-demo example with gpui-demo-test from the current clean
source first. Use a fresh output directory. This records guest/composed PNGs
and actual depth/state sidecars; archive and review results before claiming
qualification. Use --smooth-review for antialiased coverage: it checks native
state, geometry and outline preflight, and skips the incompatible binary pixel
oracle. It is not a native Macintosh oracle or production ownership test.
"""
import argparse,hashlib,importlib.util,json,pathlib,struct,subprocess
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('output',type=pathlib.Path)
parser.add_argument('--multiline', action='store_true', help='capture the 16-case selected two-line matrix')
parser.add_argument('--smooth-review', action='store_true', help='record smooth coverage/state evidence without the binary ink oracle')
parser.add_argument('--spacing', choices=['normal','condensed','extended','both'], default='normal', help='guest Option-key spacing style before capture')
parser.add_argument('--binary', type=pathlib.Path, help='explicit gpui-menu-demo build artifact; defaults to target/debug/examples/gpui-menu-demo')
args=parser.parse_args()
if args.spacing != 'normal' and not args.smooth_review:parser.error('spacing style capture requires --smooth-review; the binary ink oracle is not qualified for these styles')
root=pathlib.Path(__file__).resolve().parents[2];out=args.output.resolve()
if out.exists() and any(out.iterdir()):parser.error('use a fresh output directory; do not restart a live capture job')
out.mkdir(parents=True,exist_ok=True)
binary=args.binary.resolve() if args.binary is not None else root/'target/debug/examples/gpui-menu-demo'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
assert not subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True).strip(), 'capture from committed clean source'
binary_hash=sha(binary)
fixture=root/'tests/toolbox-showcase/toolbox-showcase.sit'
fixture_hash=sha(fixture)
v=None
if not args.smooth_review:
 s=importlib.util.spec_from_file_location('verify',root/'tests/toolbox-showcase/verify-gpui-styled-text-ink.py');v=importlib.util.module_from_spec(s);s.loader.exec_module(v)
pinned_files=['src/bin/gpui_demo.rs','src/bin/gpui_demo_coverage.rs','src/bin/gpui_demo_text.rs','src/bin/gpui_demo_frames.rs','src/systems/macintosh/quickdraw/text.rs','src/systems/macintosh/quickdraw/fonts/outline.rs','src/systems/macintosh/quickdraw/fonts/style.rs','src/systems/macintosh/quickdraw/fonts/mod.rs','tests/toolbox-showcase/capture-gpui-styled-text-matrix.py','tests/toolbox-showcase/verify-gpui-styled-text-ink.py']
source_hashes={name:sha(root/name) for name in pinned_files}
def check_pins():
 assert sha(binary)==binary_hash,'capture executable changed during qualification'
 assert sha(fixture)==fixture_hash,'guest fixture changed during qualification'
 assert all(sha(root/name)==digest for name,digest in source_hashes.items()),'pinned capture source changed'
def png_size(path):
 with path.open('rb') as png:header=png.read(24)
 assert header[:8]==b'\x89PNG\r\n\x1a\n' and header[12:16]==b'IHDR',path
 return list(struct.unpack('>II',header[16:24]))
manifest={'source_commit':source,'capture_binary_sha256':binary_hash,'fixture':'tests/toolbox-showcase/toolbox-showcase.sit','fixture_sha256':fixture_hash,'field_bounds':[126,74,164,561],'scope':'Public mixed-style field raster only; no production ownership, general themes/backgrounds, GPUI pointer mapping or native host observer qualification','cases':[],'complete':False,'matrix_kind':'multiline' if args.multiline else 'single-line','compositor':'shared Demo renderer'}
manifest['source_sha256']=source_hashes
manifest['runtime_cpu_evidence']='active application runtime'
manifest['smooth_review']=args.smooth_review
manifest['spacing_style']=args.spacing
if args.smooth_review:manifest['scope']='Shared smooth field preflight, native guest state and geometry; requires explicit composed visual review. No binary ink oracle, all font fidelity, physical host input, production performance or release claim.'
configs=[('inactive','inactive',None,'--capture-styled-text-edit-ink'),*[( 'selection',state,None,flag) for state,flag in [('selected','--capture-styled-text-edit-selected'),('suspended','--capture-styled-text-edit-selected-suspended'),('resumed','--capture-styled-text-edit-selected-resumed')]],*[( 'caret',state,offset,'--capture-styled-text-edit-caret') for offset in [0,26] for state in ['visible','blink-off','suspended','resumed']]]
if args.multiline: configs=[('multiline','selected',None,'--capture-styled-text-edit-multiline')]
total=16 if args.multiline else 192
for mode,depth,flags in [('ppc16',16,['--prefer-powerpc']),('ppc8',8,['--prefer-powerpc','--screen-depth','8']),('mono',1,['--screen-depth','1']),('colour',8,['--screen-depth','8'])]:
 for kind,state,offset,flag in configs:
  for scale in [0.75,1,1.5,2]:
   check_pins()
   name=f'{kind}-{mode}-{state}-{offset if offset is not None else "none"}-{scale}'
   path=out/(name+'.png');cmd=[str(binary),str(root/manifest['fixture']),*flags,'--capture-scale',str(scale),flag,str(path)]
   cmd+=['--capture-styled-spacing',args.spacing]
   if kind=='caret':cmd+=['--capture-styled-caret-offset',str(offset),'--capture-styled-caret-state',state]
   result=subprocess.run(cmd,capture_output=True,text=True)
   if result.returncode:
    (out/(name+'.failure.txt')).write_text(result.stdout+result.stderr)
    print('FAILED '+name+'\n'+result.stderr,flush=True);raise SystemExit(result.returncode)
   check_pins()
   evidence=json.loads(path.with_suffix('.json').read_text())
   assert type(evidence['runtime_powerpc']) is bool
   assert evidence['runtime_powerpc']==mode.startswith('ppc'),(name,evidence)
   assert evidence['depth']==depth,(name,evidence)
   assert evidence['drawing_intact'] and evidence['active']==(state not in ['inactive','suspended']),(name,evidence)
   assert evidence.get('compositor')=='shared Demo renderer'
   assert bool(evidence.get('multiline',False))==(kind=='multiline')
   expected=[0,31] if kind=='multiline' else [0,26] if kind=='selection' else [offset or 0,offset or 0]
   assert evidence['selection']==expected,(name,evidence)
   if kind=='caret':
    assert evidence['caret_state']==state and evidence['insertion_offset']==offset
    if state!='suspended':assert evidence['caret_visible']==(state!='blink-off'),(name,evidence)
   case={'kind':kind,'mode':mode,'actual_depth':depth,'state':state,'insertion_offset':offset,'scale':scale,'device_scale':2,'rendered':path.name,'guest':path.with_suffix('.guest.png').name,'evidence':path.with_suffix('.json').name,'command':cmd,'rendered_sha256':sha(path),'guest_sha256':sha(path.with_suffix('.guest.png')),'evidence_sha256':sha(path.with_suffix('.json'))}
   if args.smooth_review:
    assert evidence['view']==manifest['field_bounds'],(name,evidence['view'])
    assert evidence['smooth_raster_support_1_through_8']==[True]*8,(name,evidence)
    assert evidence['style_runs'],(name,evidence)
    assert png_size(path)==[round(800*scale*2),round(600*scale*2)],name
    assert png_size(path.with_suffix('.guest.png'))==[800,600],name
    case['smooth_raster_support_1_through_8']=evidence['smooth_raster_support_1_through_8']
    case['rendered_dimensions']=png_size(path)
    case['device_scale_evidence']='inferred from composed dimensions and requested scene scale'
    case['binary_ink_oracle']='not applied to antialiased coverage'
   else:v.verify_case(out,case,manifest['field_bounds'])
   manifest['cases'].append(case)
   (out/'progress.json').write_text(json.dumps(manifest,indent=2)+'\n')
   print(f'{len(manifest["cases"])}/{total} {"STATE/GEOMETRY" if args.smooth_review else "PASS"} {name} depth={depth}',flush=True)
assert len(manifest['cases'])==total
manifest['complete']=True;(out/'progress.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(f'All {total} shared Demo styled field captures completed; {"smooth appearance awaits explicit review" if args.smooth_review else "binary ink oracle passed"}',flush=True)
