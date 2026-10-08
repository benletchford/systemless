Place SheepShaver / MacEmu patch files here when building a patched `sheepshaver-play`
image.

The intended workflow is:

1. Keep your patch series on the host, outside the container.
2. Build the image with `build_play_image.sh`.
3. Let the Docker build clone `https://github.com/cebix/macemu`, copy these
   patch files into the build context, and apply them before linking and compiling
   SheepShaver.

Supported patch file extensions:

- `.patch`
- `.diff`

Patches are applied in sorted filename order with `git apply`.
