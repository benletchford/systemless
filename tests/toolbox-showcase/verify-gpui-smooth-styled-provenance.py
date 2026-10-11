#!/usr/bin/env python3
"""Verify smooth capture provenance/state/geometry; does not verify glyph pixels."""
import argparse,hashlib,json,pathlib,struct

def verify(directory,partial=False):
 directory=pathlib.Path(directory)
 manifest=json.loads((directory/'progress.json').read_text())
 assert manifest['smooth_review'] is True
 assert manifest['compositor']=='shared Demo renderer'
 def digest(value,length):return isinstance(value,str) and len(value)==length and set(value)<=set('0123456789abcdef')
 assert digest(manifest['source_commit'],40) and manifest['source_sha256']
 assert all(digest(value,64) for value in manifest['source_sha256'].values())
 assert digest(manifest['capture_binary_sha256'],64) and digest(manifest['fixture_sha256'],64)
 spacing_style=manifest.get('spacing_style','normal')
 assert spacing_style in ['normal','condensed','extended','both']
 spacing={'normal':0,'condensed':32,'extended':64,'both':96}[spacing_style]
 multiline=manifest['matrix_kind']=='multiline'
 assert manifest['matrix_kind'] in ['multiline','single-line']
 configs=[('multiline','selected',None)] if multiline else [('inactive','inactive',None),*[('selection',s,None) for s in ['selected','suspended','resumed']],*[('caret',s,o) for o in [0,26] for s in ['visible','blink-off','suspended','resumed']]]
 expected={(kind,mode,state,offset,scale) for kind,state,offset in configs for mode in ['mono','colour','ppc8','ppc16'] for scale in [0.75,1,1.5,2]}
 seen=set()
 def path(name):
  assert pathlib.Path(name).name==name,'capture paths must be basenames'
  return directory/name
 def sha(file):return hashlib.sha256(file.read_bytes()).hexdigest()
 def dims(file):
  with file.open('rb') as png:header=png.read(24)
  assert header[:8]==b'\x89PNG\r\n\x1a\n' and header[12:16]==b'IHDR'
  return list(struct.unpack('>II',header[16:24]))
 for case in manifest['cases']:
  key=tuple(case[k] for k in ['kind','mode','state','insertion_offset','scale'])
  assert key in expected and key not in seen,key
  seen.add(key)
  for name,digest in [('rendered','rendered_sha256'),('guest','guest_sha256'),('evidence','evidence_sha256')]:assert sha(path(case[name]))==case[digest],(key,name)
  evidence=json.loads(path(case['evidence']).read_text())
  assert ('runtime_cpu_evidence' in manifest)==('runtime_powerpc' in evidence), 'runtime CPU evidence policy and sidecar must agree'
  if 'runtime_cpu_evidence' in manifest:
   assert manifest['runtime_cpu_evidence']=='active application runtime'
   assert type(evidence.get('runtime_powerpc')) is bool
   assert evidence['runtime_powerpc']==case['mode'].startswith('ppc')
  depth={'mono':1,'colour':8,'ppc8':8,'ppc16':16}[case['mode']]
  command=case['command']
  assert type(command) is list and all(type(arg) is str for arg in command)
  assert command.count('--prefer-powerpc')==int(case['mode'].startswith('ppc'))
  requested_depth=None if case['mode']=='ppc16' else str(depth)
  assert command.count('--screen-depth')==int(requested_depth is not None)
  if requested_depth is not None:assert command[command.index('--screen-depth')+1]==requested_depth
  assert ('spacing_style' in manifest)==('spacing_style' in evidence), 'spacing evidence policy and sidecar must agree'
  if 'spacing_style' in manifest:
   assert evidence['spacing_style']==spacing_style
   assert command.count('--capture-styled-spacing')==1
   assert command[command.index('--capture-styled-spacing')+1]==spacing_style
  else:assert '--capture-styled-spacing' not in command
  assert command.count('--capture-scale')==1
  assert float(command[command.index('--capture-scale')+1])==case['scale']
  assert type(case['actual_depth']) is int and type(evidence['depth']) is int
  assert case['actual_depth']==evidence['depth']==depth
  assert evidence['scale']==case['scale']
  assert evidence['drawing_intact'] is True
  assert type(evidence['active']) is bool
  assert evidence['active']==(case['state'] not in ['inactive','suspended'])
  assert evidence['compositor']=='shared Demo renderer'
  assert evidence['view']==manifest['field_bounds']==[126,74,164,561]
  assert type(evidence['multiline']) is bool and evidence['multiline']==multiline
  selection=[0,31] if multiline else [0,26] if case['kind']=='selection' else [case['insertion_offset'] or 0]*2
  assert evidence['selection']==selection
  if case['kind']=='caret':
   assert type(evidence['insertion_offset']) is int and type(evidence['caret_visible']) is bool
   assert evidence['caret_state']==case['state'] and evidence['insertion_offset']==case['insertion_offset']
   if case['state']!='suspended':assert evidence['caret_visible']==(case['state']!='blink-off')
  assert evidence['smooth_raster_support_1_through_8']==case['smooth_raster_support_1_through_8']==[True]*8
  assert all(value is True for value in evidence['smooth_raster_support_1_through_8'])
  assert all(value is True for value in case['smooth_raster_support_1_through_8'])
  runs=evidence['style_runs']
  assert all(type(run[key]) is int for run in runs for key in ['start','font','size','face'])
  assert [tuple(run[key] for key in ['start','font','size','face']) for run in runs]==[(start,font,size,face|spacing) for start,font,size,face in [
   (0,3,10,0),(7,3,12,1),(11,3,10,0),(13,4,10,0),
   (18,3,10,0),(20,4,14,2),(24,3,10,0),(27 if multiline else 26,3,10,4)]]
  assert case['binary_ink_oracle']=='not applied to antialiased coverage'
  assert dims(path(case['guest']))==[800,600]
  assert dims(path(case['rendered']))==case['rendered_dimensions']==[round(800*case['scale']*2),round(600*case['scale']*2)]
  assert case['device_scale']==2 and case['device_scale_evidence']=='inferred from composed dimensions and requested scene scale'
 assert seen
 if not partial:assert seen==expected and manifest['complete'] is True
 return len(seen)

if __name__=='__main__':
 parser=argparse.ArgumentParser(description=__doc__)
 parser.add_argument('directory',type=pathlib.Path)
 parser.add_argument('--partial',action='store_true')
 args=parser.parse_args()
 print(f'{verify(args.directory,args.partial)} smooth capture provenance/state/geometry checks passed; glyph appearance requires explicit review')
