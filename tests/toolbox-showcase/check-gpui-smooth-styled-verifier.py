#!/usr/bin/env python3
"""Exercise provenance/state rejection; no glyph-pixel oracle is claimed."""
import argparse,copy,hashlib,importlib.util,json,pathlib,sys,tempfile
sys.dont_write_bytecode=True
checker=pathlib.Path(__file__).with_name('verify-gpui-smooth-styled-provenance.py')
spec=importlib.util.spec_from_file_location('provenance',checker);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('source',type=pathlib.Path);parser.add_argument('--report',type=pathlib.Path);args=parser.parse_args()
base=json.loads((args.source/'progress.json').read_text());base['cases']=base['cases'][:1];base['complete']=False
case=base['cases'][0];original=json.loads((args.source/case['evidence']).read_text())
checks={
 'actual-depth':lambda m,c,e:e.update(depth=8),
 'scene-scale':lambda m,c,e:(c.update(scale=1),e.update(scale=1)),
 'active-state':lambda m,c,e:e.update(active=False),
 'selection':lambda m,c,e:e.update(selection=[0,26]),
 'smooth-preflight':lambda m,c,e:(c.update(smooth_raster_support_1_through_8=[False]*8),e.update(smooth_raster_support_1_through_8=[False]*8)),
 'rendered-hash':lambda m,c,e:c.update(rendered_sha256='0'*64),
 'case-set':lambda m,c,e:c.update(mode='invalid'),
 'source-digest':lambda m,c,e:m.update(source_commit='x'*40),
 'boolean-active-type':lambda m,c,e:e.update(active=1),
 'boolean-preflight-type':lambda m,c,e:(c.update(smooth_raster_support_1_through_8=[1]*8),e.update(smooth_raster_support_1_through_8=[1]*8)),
}
with tempfile.TemporaryDirectory(prefix='systemless-styled-verifier-') as temporary:
 out=pathlib.Path(temporary)
 for key in ['rendered','guest']:(out/case[key]).symlink_to((args.source/case[key]).resolve())
 def check(change):
  manifest=copy.deepcopy(base);evidence=copy.deepcopy(original);change(manifest,manifest['cases'][0],evidence)
  ep=out/case['evidence'];ep.write_text(json.dumps(evidence))
  manifest['cases'][0]['evidence_sha256']=hashlib.sha256(ep.read_bytes()).hexdigest()
  (out/'progress.json').write_text(json.dumps(manifest));return module.verify(out,True)
 assert check(lambda m,c,e:None)==1
 for name,change in checks.items():
  try:check(change)
  except AssertionError:print('Rejected '+name)
  else:raise AssertionError('accepted altered '+name)
report={'verifier_sha256':hashlib.sha256(checker.read_bytes()).hexdigest(),'positive_partial_case':True,'rejected':list(checks),'scope':'Provenance/state/geometry validation; does not verify glyph appearance.'}
if args.report:args.report.write_text(json.dumps(report,indent=2)+'\n')
