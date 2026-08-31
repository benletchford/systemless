# Toolbox Showcase fixture

`ToolboxShowcase.sit` contains one fat classic Macintosh application. MPW
compiles `src/main.c` twice and combines the classic 68K `CODE` resources with
a native PowerPC PEF data fork and `cfrg` resource. Both slices therefore run
the same event loop and page implementations.

The application is a human-interactable playbook with six pages:

1. Overview connects the managers exercised by the fixture.
2. QuickDraw covers geometry, colors, patterns, pen modes, regions, clipping,
   polygons, styled text, and `CopyBits`.
3. Controls covers buttons, a checkbox, radio buttons, and two scroll bars.
4. TextEdit provides editable text plus Cut, Copy, Paste, and Clear commands.
5. Windows opens a second document window for activation, layering, dragging,
   growing, zooming, updating, and closing.
6. Resources & Events inventories the resource-loaded interface and event
   dispatch exercised throughout the application.

The Pages menu, Previous and Next buttons, number keys 1 through 6, and arrow
keys all navigate or scroll the playbook. The Demo menu exposes persistent
checkmarks for checkbox, scrolling, and text-editing state so automated probes
can inspect the same behavior a person sees.

## Run it in Systemless

From the repository root, run the 68K slice:

```sh
cargo run --release -- tests/fixtures/toolbox-showcase/ToolboxShowcase.sit
```

Run the native PowerPC slice from the same archive:

```sh
cargo run --release -- --prefer-powerpc tests/fixtures/toolbox-showcase/ToolboxShowcase.sit
```

The CI integration test launches both slices, navigates every page, toggles a
control, moves the document scroll bar, edits TextEdit content, opens and
closes the companion window, and exits through the File menu.

## Rebuild the committed archive

The builder requires Docker and downloads MPW 3.5 at image-build time. No MPW
software is committed to this repository. The small Rust packager uses the
published `stuffit` crate and fixes archive and PEF timestamps so identical
source produces identical archive bytes. Its manifest is stored as
`Cargo.toml.in` and copied into the temporary workspace so Cargo includes every
rebuild input in the published source package instead of excluding a nested
package.

Update the committed artifact after changing source:

```sh
./tests/fixtures/toolbox-showcase/build.sh --update
```

Verify a clean rebuild without replacing it:

```sh
./tests/fixtures/toolbox-showcase/build.sh --check
```

Docker layers, the temporary MPW workspace, compiler objects, PEF intermediates,
raw forks, and the packager `target/` directory are build products and must not
be committed.

## BasiliskII and SheepShaver oracle pass

Expand `ToolboxShowcase.sit` with a resource-fork-preserving tool or expand it
inside the guest. BasiliskII runs the 68K `CODE` slice; SheepShaver runs the
native PowerPC fragment. Use a 640 by 480 display and perform this common pass:

1. Confirm the Overview architecture label matches the emulator CPU.
2. Visit pages 1 through 6 from the Pages menu and with Previous/Next.
3. On Controls, toggle the checkbox and both radio buttons, drag the horizontal
   scroll box, and use the right-side vertical scroll bar.
4. On TextEdit, select and type text, then exercise Cut, Copy, Paste, and Clear.
5. On Windows, open the companion window, activate each window, then drag,
   resize, zoom, and close the companion.
6. Open the Palette submenu, select each color, and open and dismiss the About
   alert.
7. Quit from File and compare screenshots and interaction results at each
   checkpoint. Meaningful differences are runtime defects; the emulator output
   is the behavioral oracle.
