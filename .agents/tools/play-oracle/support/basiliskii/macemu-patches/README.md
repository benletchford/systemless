Place BasiliskII patch files here when building a patched `basiliskii-play`
image.

The intended workflow is:

1. Keep your patch series on the host, outside the container.
2. Author or refresh those patches against the local reference checkout in
   a checkout of `https://github.com/cebix/macemu` if that is convenient.
3. Build the image with `build_play_image.sh`.
4. Let the Docker build clone `https://github.com/cebix/macemu`, copy these
   patch files into the build context, and apply them before compiling
   BasiliskII.

Supported patch file extensions:

- `.patch`
- `.diff`

Patches are applied in sorted filename order with `git apply`.
