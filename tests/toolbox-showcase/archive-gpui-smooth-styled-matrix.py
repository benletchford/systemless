#!/usr/bin/env python3
"""Archive a complete smooth matrix losslessly, preserving explicit review scope."""
import argparse,copy,hashlib,importlib.util,json,pathlib,shutil,sys
from PIL import Image
sys.dont_write_bytecode=True
checker=pathlib.Path(__file__).with_name('verify-gpui-smooth-styled-provenance.py')
spec=importlib.util.spec_from_file_location('provenance',checker)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()

def archive(source,destination):
 source=pathlib.Path(source);destination=pathlib.Path(destination)
 count=module.verify(source)
 assert not destination.exists(),'use a fresh archive directory'
 manifest=json.loads((source/'progress.json').read_text())
 manifest=copy.deepcopy(manifest)
 destination.mkdir(parents=True)
 manifest['archive_tools']={'verifier_sha256':sha(checker),'archiver_sha256':sha(pathlib.Path(__file__)),'scope':'PNG compression only; decoded RGBA equality asserted for every image. Tools are pinned separately from the original capture renderer.'}
 originals={}
 for case in manifest['cases']:
  for key in ['rendered','guest']:
   src=source/case[key];dst=destination/case[key]
   originals[case[key]]=case[key+'_sha256']
   with Image.open(src) as im:
    rgba=im.convert('RGBA');pixels=rgba.tobytes();dimensions=rgba.size
    rgba.save(dst,compress_level=9,optimize=True)
   with Image.open(dst) as im:assert im.convert('RGBA').tobytes()==pixels and im.size==dimensions
   case['capture_'+key+'_sha256']=case[key+'_sha256']
   case[key+'_sha256']=sha(dst)
  case['decoded_rgba_preserved']=True
  shutil.copy2(source/case['evidence'],destination/case['evidence'])
 (destination/'progress.json').write_text(json.dumps(manifest,indent=2)+'\n')
 review_path=source/'visual-review.json'
 if review_path.exists():
  review=json.loads(review_path.read_text());seen=set()
  for image in review['images']:
   name=image['file']
   assert name in originals and name not in seen
   seen.add(name)
   assert image['sha256']==originals[name],'review must pin the actual reviewed capture'
   image['capture_sha256']=image['sha256'];image['sha256']=sha(destination/name)
  (destination/'review.json').write_text(json.dumps(review,indent=2)+'\n')
 assert module.verify(destination)==count
 print(f'Archived {count} complete cases; provenance/state/geometry checked before/after; all decoded RGBA preserved. Review scope remains explicit.')

if __name__=='__main__':
 parser=argparse.ArgumentParser(description=__doc__)
 parser.add_argument('source',type=pathlib.Path);parser.add_argument('destination',type=pathlib.Path)
 args=parser.parse_args();archive(args.source,args.destination)
