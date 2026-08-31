# Changelog

## [0.27.1](https://github.com/benletchford/systemless/compare/v0.27.0...v0.27.1) (2026-08-31)


### Bug Fixes

* **control:** render PowerPC radio buttons ([#1085](https://github.com/benletchford/systemless/issues/1085)) ([6f1aaf2](https://github.com/benletchford/systemless/commit/6f1aaf2e7aac4e320dd3a772b33e67312902e193))

## [0.27.0](https://github.com/benletchford/systemless/compare/v0.26.0...v0.27.0) (2026-08-31)


### Features

* expand the Toolbox showcase ([#1079](https://github.com/benletchford/systemless/issues/1079)) ([8dfe308](https://github.com/benletchford/systemless/commit/8dfe3082fcb17cfff63e33fda44efcc7294035b9))


### Bug Fixes

* preserve and exercise indexed palettes ([#1083](https://github.com/benletchford/systemless/issues/1083)) ([722a3c3](https://github.com/benletchford/systemless/commit/722a3c3ddc8961b6d73d236b494e941f9624b345))

## [0.26.0](https://github.com/benletchford/systemless/compare/v0.25.0...v0.26.0) (2026-08-31)


### Features

* **desktop:** draw the guest cursor as the host pointer on macOS ([#992](https://github.com/benletchford/systemless/issues/992)) ([5747b14](https://github.com/benletchford/systemless/commit/5747b14db5a1d0f9a3fc17adaedfe8bd711dcd9a))


### Bug Fixes

* **dialog:** make CountDITL count only the target dialog ([#1055](https://github.com/benletchford/systemless/issues/1055)) ([91a4da1](https://github.com/benletchford/systemless/commit/91a4da1bb8aa459327052eddc303256c42db1483))
* **event:** return documented VBL slot error ([#1063](https://github.com/benletchford/systemless/issues/1063)) ([f310753](https://github.com/benletchford/systemless/commit/f310753965bb3800d4a90b8843edac988dc89c68))
* **ppc:** reclaim Mixed Mode routine descriptors ([#1059](https://github.com/benletchford/systemless/issues/1059)) ([cebadf5](https://github.com/benletchford/systemless/commit/cebadf5a224eb21c9bbdf190909cb982a4e028fb))
* **quickdraw:** consume the complete StdOpcode frame ([#1061](https://github.com/benletchford/systemless/issues/1061)) ([2262fb2](https://github.com/benletchford/systemless/commit/2262fb28bb43a6b7ae0a5531d06ebdd9d8c62d14))
* **quicktime:** parse Cinepak 24-bit lengths ([#1068](https://github.com/benletchford/systemless/issues/1068)) ([61abf4c](https://github.com/benletchford/systemless/commit/61abf4c55fed0f8032ea66201b1d7b1f87f3839f))
* **sane:** preserve FLOGB floating results ([#1066](https://github.com/benletchford/systemless/issues/1066)) ([0d3b6b9](https://github.com/benletchford/systemless/commit/0d3b6b91655d704444e6e7ee9375adfe19b7f1e1))
* **sound:** route MIDIStopTime by source identity ([#1057](https://github.com/benletchford/systemless/issues/1057)) ([0801fba](https://github.com/benletchford/systemless/commit/0801fba623b9a17e8e1c1acbe817b8551df7fad0))
* **textedit:** preserve TEStyleInsert caller registers ([#1048](https://github.com/benletchford/systemless/issues/1048)) ([a764acd](https://github.com/benletchford/systemless/commit/a764acd7a228dd26bb01cfcc52de8ed0e31458f2))
* **toolbox:** correct SystemClick pointer argument ABI ([#1074](https://github.com/benletchford/systemless/issues/1074)) ([8e00b71](https://github.com/benletchford/systemless/commit/8e00b712a70237be9d38be14c34378b92339c849))
* **toolbox:** correct SystemEvent pointer argument ABI ([#1072](https://github.com/benletchford/systemless/issues/1072)) ([fcdc676](https://github.com/benletchford/systemless/commit/fcdc6764a4076c83981c65f0fd69e463d21ee092))
* **toolbox:** correct X2Fix pointer argument ABI ([#1051](https://github.com/benletchford/systemless/issues/1051)) ([0298afb](https://github.com/benletchford/systemless/commit/0298afba916827a56ef31950f79f648056bfbacf))
* **window:** use system wctb resource for default window color table ([#1005](https://github.com/benletchford/systemless/issues/1005)) ([ec2d367](https://github.com/benletchford/systemless/commit/ec2d367f23fcd27779bf4b09f7c1bcaa8257e545))


### Performance Improvements

* **runner:** admit journal-complete Window Manager queries and TEIdle to idle-cycle proofs ([#962](https://github.com/benletchford/systemless/issues/962)) ([ecb1a5e](https://github.com/benletchford/systemless/commit/ecb1a5e3541eb74b4d808e3fbf10f6dbad6c8110))

## [0.25.0](https://github.com/benletchford/systemless/compare/v0.24.0...v0.25.0) (2026-08-30)


### Features

* **traps:** inventory HFSDispatch selectors ([#1044](https://github.com/benletchford/systemless/issues/1044)) ([3afb76a](https://github.com/benletchford/systemless/commit/3afb76a11fd8c905eeb3f17fb8404d1698fcf626))
* **traps:** inventory LoadIconCache selector ([#1047](https://github.com/benletchford/systemless/issues/1047)) ([e0d3b9f](https://github.com/benletchford/systemless/commit/e0d3b9ff7c2789184afed9389a99eb11906e76a5))

## [0.24.0](https://github.com/benletchford/systemless/compare/v0.23.23...v0.24.0) (2026-08-30)


### Features

* **traps:** inventory FontDispatch selectors ([#1021](https://github.com/benletchford/systemless/issues/1021)) ([24fb483](https://github.com/benletchford/systemless/commit/24fb483e3ced433db8e83de85f77854785238c7f))
* **traps:** inventory Pack11 selector operations ([#1031](https://github.com/benletchford/systemless/issues/1031)) ([585be8f](https://github.com/benletchford/systemless/commit/585be8f291f45a9bf2af4237728a1c87f7345332))
* **traps:** inventory Pack15 selectors ([#1023](https://github.com/benletchford/systemless/issues/1023)) ([8d666b5](https://github.com/benletchford/systemless/commit/8d666b5943b07ad3d1c9d1ed82997a393fb7c504))
* **traps:** inventory Pack2 selectors ([#1025](https://github.com/benletchford/systemless/issues/1025)) ([8ce18de](https://github.com/benletchford/systemless/commit/8ce18de4a922bd6400538a5d52e76b9c47203bec))
* **traps:** inventory Pack3 selectors ([#1019](https://github.com/benletchford/systemless/issues/1019)) ([a1aa46d](https://github.com/benletchford/systemless/commit/a1aa46dd96fe1cdfd68c4ddfe3576e811ec6c0d8))
* **traps:** inventory TEDispatch selectors ([#1037](https://github.com/benletchford/systemless/issues/1037)) ([5774fb2](https://github.com/benletchford/systemless/commit/5774fb28a7e674c4ee37ef5d1682c8292fee2f1a))
* **traps:** inventory TranslationDispatch selectors ([#1035](https://github.com/benletchford/systemless/issues/1035)) ([b881b19](https://github.com/benletchford/systemless/commit/b881b199247a3820c68e8e9e5cd0e251aac0d1c2))


### Bug Fixes

* **menu:** preserve native handle growth across 68k callbacks ([#1017](https://github.com/benletchford/systemless/issues/1017)) ([e3786b1](https://github.com/benletchford/systemless/commit/e3786b1b1c15480d2104f6b91ac7d91bd9d19ad0))
* **traps:** restore Pack11 Pascal stack discipline ([#1042](https://github.com/benletchford/systemless/issues/1042)) ([4062ef5](https://github.com/benletchford/systemless/commit/4062ef5e0c3d88b0ab09385ba1fea1c182748b7a))
* **traps:** restore Pack13 Pascal stack discipline ([#1029](https://github.com/benletchford/systemless/issues/1029)) ([622dd85](https://github.com/benletchford/systemless/commit/622dd852a35ec18273ebdfe4a7397874e19735c6))
* **traps:** route Pack9 to PPCBrowser ([#1027](https://github.com/benletchford/systemless/issues/1027)) ([f9cfba1](https://github.com/benletchford/systemless/commit/f9cfba1027835aa1233b4e208aeda9634366f13e))


### Performance Improvements

* **quickdraw:** copy CopyBits rows span by span instead of pixel by pixel ([#963](https://github.com/benletchford/systemless/issues/963)) ([73e7c7f](https://github.com/benletchford/systemless/commit/73e7c7f8bb5da787cc92b08eaf6a714719c61928))

## [0.23.23](https://github.com/benletchford/systemless/compare/v0.23.22...v0.23.23) (2026-08-30)


### Code Refactoring

* **menu:** make process context own tracking ([53e93d2](https://github.com/benletchford/systemless/commit/53e93d2c3c18716c04e34ce20b797d13ff913ef1))

## [0.23.22](https://github.com/benletchford/systemless/compare/v0.23.21...v0.23.22) (2026-08-30)


### Code Refactoring

* **event:** make process context own queue ([#1011](https://github.com/benletchford/systemless/issues/1011)) ([20432d3](https://github.com/benletchford/systemless/commit/20432d328436bbbc8d5475b5e09fa44cc89b869e))

## [0.23.21](https://github.com/benletchford/systemless/compare/v0.23.20...v0.23.21) (2026-08-30)


### Code Refactoring

* **runtime:** introduce process-owned context ([aa09e4e](https://github.com/benletchford/systemless/commit/aa09e4eb08d58f43c44e712f25399a7a6f31fecc)), closes [#1006](https://github.com/benletchford/systemless/issues/1006)

## [0.23.20](https://github.com/benletchford/systemless/compare/v0.23.19...v0.23.20) (2026-08-30)


### Bug Fixes

* **menu:** preserve native mutations across 68k callbacks ([e0983bb](https://github.com/benletchford/systemless/commit/e0983bbebb5859e51235db8bf8801650861eec77))

## [0.23.19](https://github.com/benletchford/systemless/compare/v0.23.18...v0.23.19) (2026-08-30)


### Bug Fixes

* **files:** keep relative paths on the default volume ([#999](https://github.com/benletchford/systemless/issues/999)) ([f1a347f](https://github.com/benletchford/systemless/commit/f1a347fb538b34d8c7de17b8702f928017e30cdb))

## [0.23.18](https://github.com/benletchford/systemless/compare/v0.23.17...v0.23.18) (2026-08-30)


### Bug Fixes

* **release:** include refactors in release notes ([#996](https://github.com/benletchford/systemless/issues/996)) ([e01c73a](https://github.com/benletchford/systemless/commit/e01c73a8c826098b4c46b18cd8f54ba7530f2232))


### Code Refactoring

* **menu:** use one Menu Manager for 68k and PowerPC ([#947](https://github.com/benletchford/systemless/issues/947)) ([69d911e](https://github.com/benletchford/systemless/commit/69d911eb62ec4fd8fb32d2cdefeb4f60f9fbfbc0))

## [0.23.17](https://github.com/benletchford/systemless/compare/v0.23.16...v0.23.17) (2026-08-29)


### Bug Fixes

* recycle PowerPC picture loading allocations ([#990](https://github.com/benletchford/systemless/issues/990)) ([ad826ab](https://github.com/benletchford/systemless/commit/ad826abe89a91967c41080eaf702eea5d69ee004))

## [0.23.16](https://github.com/benletchford/systemless/compare/v0.23.15...v0.23.16) (2026-08-29)


### Bug Fixes

* dispose PowerPC dialogs as windows ([0d98ec4](https://github.com/benletchford/systemless/commit/0d98ec459fb4c3704d5bdfc841b4f5ba1f7d5345))

## [0.23.15](https://github.com/benletchford/systemless/compare/v0.23.14...v0.23.15) (2026-08-29)


### Bug Fixes

* support UpperText in PowerPC Text Utilities ([#981](https://github.com/benletchford/systemless/issues/981)) ([b5a07c5](https://github.com/benletchford/systemless/commit/b5a07c5de3600a646c74f512111488f20f083abb))

## [0.23.14](https://github.com/benletchford/systemless/compare/v0.23.13...v0.23.14) (2026-08-29)


### Bug Fixes

* enumerate mounted volumes in PowerPC File Manager ([#978](https://github.com/benletchford/systemless/issues/978)) ([cfcb1fc](https://github.com/benletchford/systemless/commit/cfcb1fcd83c42f6e9a6df5ea39b61692c4e7bed6))

## [0.23.13](https://github.com/benletchford/systemless/compare/v0.23.12...v0.23.13) (2026-08-29)


### Bug Fixes

* **ppc:** route dialog events ([#975](https://github.com/benletchford/systemless/issues/975)) ([37846cb](https://github.com/benletchford/systemless/commit/37846cbf891f841a7dca355cbda44b009b3c9c9a))

## [0.23.12](https://github.com/benletchford/systemless/compare/v0.23.11...v0.23.12) (2026-08-29)


### Bug Fixes

* **ppc:** implement QuickDraw EqualPt ([#972](https://github.com/benletchford/systemless/issues/972)) ([679b545](https://github.com/benletchford/systemless/commit/679b5451a88f0f9c7e26c5cf55899651043df7a3))

## [0.23.11](https://github.com/benletchford/systemless/compare/v0.23.10...v0.23.11) (2026-08-29)


### Bug Fixes

* **ppc:** support InputSprocket element lists ([#969](https://github.com/benletchford/systemless/issues/969)) ([64dae33](https://github.com/benletchford/systemless/commit/64dae33aaa8e1595dd679339de4e76e7310fa4fe))

## [0.23.10](https://github.com/benletchford/systemless/compare/v0.23.9...v0.23.10) (2026-08-29)


### Bug Fixes

* preserve classic HFS media metadata ([9c717a8](https://github.com/benletchford/systemless/commit/9c717a81470dce178d54396c56ed6e51ca7746ba))

## [0.23.9](https://github.com/benletchford/systemless/compare/v0.23.8...v0.23.9) (2026-08-29)


### Bug Fixes

* restore kiosk chrome and indexed palettes ([131fd1a](https://github.com/benletchford/systemless/commit/131fd1afdb3f21f9431b80460a7f27e46ff59040))

## [0.23.8](https://github.com/benletchford/systemless/compare/v0.23.7...v0.23.8) (2026-08-28)


### Bug Fixes

* **cpu:** preserve 68040 address error frames ([#956](https://github.com/benletchford/systemless/issues/956)) ([f92aee5](https://github.com/benletchford/systemless/commit/f92aee572e88d7d0176ee02fce05769dcde105e7))

## [0.23.7](https://github.com/benletchford/systemless/compare/v0.23.6...v0.23.7) (2026-08-28)


### Bug Fixes

* **deps:** update m68k to 0.11.5 ([#951](https://github.com/benletchford/systemless/issues/951)) ([d993950](https://github.com/benletchford/systemless/commit/d993950314b2cc34635045458a2cc79db83b6c2b))

## [0.23.6](https://github.com/benletchford/systemless/compare/v0.23.5...v0.23.6) (2026-08-28)


### Bug Fixes

* **cpu:** restore corrected m68k and ppc releases ([#945](https://github.com/benletchford/systemless/issues/945)) ([2c1dd5d](https://github.com/benletchford/systemless/commit/2c1dd5d1b053b0db2571b39be5463ab5f55a2280))

## [0.23.5](https://github.com/benletchford/systemless/compare/v0.23.4...v0.23.5) (2026-08-28)


### Bug Fixes

* **cpu:** roll back dependency regressions ([#938](https://github.com/benletchford/systemless/issues/938)) ([53858dc](https://github.com/benletchford/systemless/commit/53858dcbd7ef63818e7297a1906bc8707c8c3631)), closes [#937](https://github.com/benletchford/systemless/issues/937)

## [0.23.4](https://github.com/benletchford/systemless/compare/v0.23.3...v0.23.4) (2026-08-28)


### Bug Fixes

* **runner:** wait-identity proofs compare architectural state only ([#859](https://github.com/benletchford/systemless/issues/859)) ([c55797b](https://github.com/benletchford/systemless/commit/c55797b43173fc48b6e42d3a0ef672e26ebb4769))


### Performance Improvements

* **memory:** journal write probes by 32-bit unit and keep bulk stores on their fast paths ([#861](https://github.com/benletchford/systemless/issues/861)) ([2dc4151](https://github.com/benletchford/systemless/commit/2dc4151e67aa48fa71bdd61137e92d37524057b0))
* **memory:** reuse the write-probe journal and hash it by address ([#857](https://github.com/benletchford/systemless/issues/857)) ([a6228a5](https://github.com/benletchford/systemless/commit/a6228a52e5c14068d823fb9cbf9c0906d0387f0f))
* **quickdraw:** no per-pixel region test for rectangular regions in CopyBits ([#858](https://github.com/benletchford/systemless/issues/858)) ([28556d4](https://github.com/benletchford/systemless/commit/28556d45f6751f404d8805cc252e7b42c47f6d78))
* **render:** row-blit 8bpp text and column-fill vertical borders ([#860](https://github.com/benletchford/systemless/issues/860)) ([bfea9cd](https://github.com/benletchford/systemless/commit/bfea9cd16a2dc815a5cbf4c685ebcaf923268666))
* **render:** row-bulk the 8bpp draw primitives ([#853](https://github.com/benletchford/systemless/issues/853)) ([308c221](https://github.com/benletchford/systemless/commit/308c221065ad974bf6c602756fe4494c2575a85b))

## [0.23.3](https://github.com/benletchford/systemless/compare/v0.23.2...v0.23.3) (2026-08-28)


### Bug Fixes

* **event:** use SysEvtMask as the posting authority ([b9705a7](https://github.com/benletchford/systemless/commit/b9705a7bc175f8e23c9e5ee2f5d0ecdb48ef4ca1))

## [0.23.2](https://github.com/benletchford/systemless/compare/v0.23.1...v0.23.2) (2026-08-28)


### Bug Fixes

* **event:** share the event queue across CPU backends ([#928](https://github.com/benletchford/systemless/issues/928)) ([7ffc1fd](https://github.com/benletchford/systemless/commit/7ffc1fdd441c8593fc5af6b21be7741c62b4a297))

## [0.23.1](https://github.com/benletchford/systemless/compare/v0.23.0...v0.23.1) (2026-08-28)


### Bug Fixes

* **memory:** share input globals across CPU backends ([#925](https://github.com/benletchford/systemless/issues/925)) ([86b63b6](https://github.com/benletchford/systemless/commit/86b63b6553ab76c2b70964e27d2782dd8f31b9bf))

## [0.23.0](https://github.com/benletchford/systemless/compare/v0.22.0...v0.23.0) (2026-08-28)


### Features

* **memory:** share runtime globals across CPU backends ([#922](https://github.com/benletchford/systemless/issues/922)) ([1c7735f](https://github.com/benletchford/systemless/commit/1c7735fb81897fa5fa3d0cd892838147a2bf7d42))

## [0.22.0](https://github.com/benletchford/systemless/compare/v0.21.6...v0.22.0) (2026-08-28)


### Features

* **memory:** add shared cross-architecture guest address space ([#919](https://github.com/benletchford/systemless/issues/919)) ([89442a5](https://github.com/benletchford/systemless/commit/89442a5b0e9f31b05b37ff15e389110f062fb8e2))

## [0.21.6](https://github.com/benletchford/systemless/compare/v0.21.5...v0.21.6) (2026-08-28)


### Bug Fixes

* **ppc:** deliver unmapped memory exceptions ([#916](https://github.com/benletchford/systemless/issues/916)) ([bee520e](https://github.com/benletchford/systemless/commit/bee520e2156b7edbf1c24c9ac859822ea5118d8d))

## [0.21.5](https://github.com/benletchford/systemless/compare/v0.21.4...v0.21.5) (2026-08-28)


### Bug Fixes

* **menu:** add PPC MenuChoice support ([#901](https://github.com/benletchford/systemless/issues/901)) ([a691eed](https://github.com/benletchford/systemless/commit/a691eed85bc05173ba14cb046122ef73fc5c26f9))

## [0.21.4](https://github.com/benletchford/systemless/compare/v0.21.3...v0.21.4) (2026-08-28)


### Bug Fixes

* **ppc:** deliver native application exceptions ([#912](https://github.com/benletchford/systemless/issues/912)) ([b682e7f](https://github.com/benletchford/systemless/commit/b682e7f5f003a73c6202b4acbb8728ddfc6f3698))

## [0.21.3](https://github.com/benletchford/systemless/compare/v0.21.2...v0.21.3) (2026-08-27)


### Reverts

* remove runtime route inventory from master ([#907](https://github.com/benletchford/systemless/issues/907)) ([dca0110](https://github.com/benletchford/systemless/commit/dca0110c949ee7ded4b403dc236bc29c26038a14))

## [0.21.2](https://github.com/benletchford/systemless/compare/v0.21.1...v0.21.2) (2026-08-27)


### Bug Fixes

* **menu:** preserve PPC submenu title semantics ([#898](https://github.com/benletchford/systemless/issues/898)) ([67946fb](https://github.com/benletchford/systemless/commit/67946fb8aff01e82cce372558123f570465a6c12))

## [0.21.1](https://github.com/benletchford/systemless/compare/v0.21.0...v0.21.1) (2026-08-27)


### Bug Fixes

* **menu:** track nested PPC submenus ([#895](https://github.com/benletchford/systemless/issues/895)) ([ba73e2c](https://github.com/benletchford/systemless/commit/ba73e2c28c3adaf4ee69ecd69fbe0f80aa159166))

## [0.21.0](https://github.com/benletchford/systemless/compare/v0.20.17...v0.21.0) (2026-08-27)


### Features

* **menu:** support styled and icon PPC items ([#892](https://github.com/benletchford/systemless/issues/892)) ([16dd023](https://github.com/benletchford/systemless/commit/16dd02336365f39f2f1e27643617d781b9f9e3f1))

## [0.20.17](https://github.com/benletchford/systemless/compare/v0.20.16...v0.20.17) (2026-08-27)


### Bug Fixes

* **ppc:** track hierarchical submenus ([#889](https://github.com/benletchford/systemless/issues/889)) ([578d6a3](https://github.com/benletchford/systemless/commit/578d6a3385970fa300ff9adccdb0c8331463271d))

## [0.20.16](https://github.com/benletchford/systemless/compare/v0.20.15...v0.20.16) (2026-08-27)


### Bug Fixes

* **ppc:** render standard menu item chrome ([#886](https://github.com/benletchford/systemless/issues/886)) ([bdbc688](https://github.com/benletchford/systemless/commit/bdbc68860fd4535e844f7ee6393b1324ca24a9ea))

## [0.20.15](https://github.com/benletchford/systemless/compare/v0.20.14...v0.20.15) (2026-08-27)


### Bug Fixes

* **68k:** retain window grow tracking ([80a4ef2](https://github.com/benletchford/systemless/commit/80a4ef20667769afdaa8eadc2606bdfb9cab4592)), closes [#882](https://github.com/benletchford/systemless/issues/882)

## [0.20.14](https://github.com/benletchford/systemless/compare/v0.20.13...v0.20.14) (2026-08-27)


### Bug Fixes

* **ppc:** retain window grow tracking ([#880](https://github.com/benletchford/systemless/issues/880)) ([b7bb243](https://github.com/benletchford/systemless/commit/b7bb2434287fd38870fd1447a417b5c72939a76d))

## [0.20.13](https://github.com/benletchford/systemless/compare/v0.20.12...v0.20.13) (2026-08-27)


### Bug Fixes

* **ppc:** retain window drag tracking ([#877](https://github.com/benletchford/systemless/issues/877)) ([ac34046](https://github.com/benletchford/systemless/commit/ac340467aa59c9358a010b3fe8b8a6321755f29c))

## [0.20.12](https://github.com/benletchford/systemless/compare/v0.20.11...v0.20.12) (2026-08-27)


### Bug Fixes

* **ppc:** retain close box tracking ([#874](https://github.com/benletchford/systemless/issues/874)) ([b6a63c2](https://github.com/benletchford/systemless/commit/b6a63c2a5692505c6c9714f9df154664d2b83ce0))

## [0.20.11](https://github.com/benletchford/systemless/compare/v0.20.10...v0.20.11) (2026-08-27)


### Bug Fixes

* **ppc:** synchronize window activation chrome ([dd1c399](https://github.com/benletchford/systemless/commit/dd1c39993b0c566e4fe4000eab4a9f59fdf447e7))

## [0.20.10](https://github.com/benletchford/systemless/compare/v0.20.9...v0.20.10) (2026-08-27)


### Bug Fixes

* **ppc:** honor WIND color window state ([c247da7](https://github.com/benletchford/systemless/commit/c247da75cdb39b458347ea66daed933cbf2de439))

## [0.20.9](https://github.com/benletchford/systemless/compare/v0.20.8...v0.20.9) (2026-08-27)


### Bug Fixes

* **ppc:** switch tracked menu bar titles ([bd0fa9a](https://github.com/benletchford/systemless/commit/bd0fa9a0a82770265882d46137a4332a6420d33b))

## [0.20.8](https://github.com/benletchford/systemless/compare/v0.20.7...v0.20.8) (2026-08-27)


### Bug Fixes

* **ppc:** track popup menus until mouse-up ([d49dbe6](https://github.com/benletchford/systemless/commit/d49dbe656d8aede595d740a7868e0a01e7fc8f02))

## [0.20.7](https://github.com/benletchford/systemless/compare/v0.20.6...v0.20.7) (2026-08-26)


### Bug Fixes

* support PPC packed indexed display depths ([17fb5af](https://github.com/benletchford/systemless/commit/17fb5afdc7c475c8572b5ce4b8eb502890c7af02))

## [0.20.6](https://github.com/benletchford/systemless/compare/v0.20.5...v0.20.6) (2026-08-26)


### Bug Fixes

* **ppc:** keep display depth state coherent ([500a700](https://github.com/benletchford/systemless/commit/500a700a3ab32c56d2e61903706d98a8b55b6f24))

## [0.20.5](https://github.com/benletchford/systemless/compare/v0.20.4...v0.20.5) (2026-08-26)


### Bug Fixes

* **quickdraw:** grow color tables before depth changes ([528d7e9](https://github.com/benletchford/systemless/commit/528d7e9007737de9451af19ebd97127ad395f07e))

## [0.20.4](https://github.com/benletchford/systemless/compare/v0.20.3...v0.20.4) (2026-08-26)


### Bug Fixes

* **menu:** restore packed four-bit pixels ([01d3ff6](https://github.com/benletchford/systemless/commit/01d3ff6ba76e71aad1d50dcc58df3c6d4d3d0efd))

## [0.20.3](https://github.com/benletchford/systemless/compare/v0.20.2...v0.20.3) (2026-08-26)


### Bug Fixes

* **ppc:** preserve hierarchical menu lists ([79e0963](https://github.com/benletchford/systemless/commit/79e096343fd1b09d269197603561f40438e988a7))

## [0.20.2](https://github.com/benletchford/systemless/compare/v0.20.1...v0.20.2) (2026-08-26)


### Bug Fixes

* complete powerpc drawsprocket compatibility ([#835](https://github.com/benletchford/systemless/issues/835)) ([174c117](https://github.com/benletchford/systemless/commit/174c1173a294e11d6fa3d5c8204c2d6f6e676372))
* restore authentic classic window and menu behavior ([#834](https://github.com/benletchford/systemless/issues/834)) ([768bf31](https://github.com/benletchford/systemless/commit/768bf31e7d405d33dd46cccc57412bfd67b08e4c))

## [0.20.1](https://github.com/benletchford/systemless/compare/v0.20.0...v0.20.1) (2026-08-25)


### Bug Fixes

* **loader:** apply Retro68 RELA relocations ([44e0179](https://github.com/benletchford/systemless/commit/44e0179ac46cf61d7cfd2b86a4cf3b21a2a4ce4b))

## [0.20.0](https://github.com/benletchford/systemless/compare/v0.19.1...v0.20.0) (2026-08-25)


### Features

* **headless:** replay input from a script scheduled by instruction count ([#824](https://github.com/benletchford/systemless/issues/824)) ([552efff](https://github.com/benletchford/systemless/commit/552efff23d06e9883f67aff9bced42c375995bd8))


### Bug Fixes

* preserve indexed artwork during palette fades ([#777](https://github.com/benletchford/systemless/issues/777)) ([ed3bda9](https://github.com/benletchford/systemless/commit/ed3bda92ace0a4fc677897605b68c76589bad4f6))
* **quickdraw:** never hand out SysZone as a GrafPtr when no port is current ([#825](https://github.com/benletchford/systemless/issues/825)) ([142c725](https://github.com/benletchford/systemless/commit/142c7251c4295ff9a54a503c8f14daea0d2648c7))

## [0.19.1](https://github.com/benletchford/systemless/compare/v0.19.0...v0.19.1) (2026-08-25)


### Bug Fixes

* complete Return to Dark Castle PowerPC compatibility ([c6e78c0](https://github.com/benletchford/systemless/commit/c6e78c00c5cea2e3937968c9a021e2c272218242))

## [0.19.0](https://github.com/benletchford/systemless/compare/v0.18.25...v0.19.0) (2026-08-25)


### Features

* **disk:** add native classic HFS reader ([#821](https://github.com/benletchford/systemless/issues/821)) ([a35b6ee](https://github.com/benletchford/systemless/commit/a35b6ee871a0cc0243db2920b811db2e9ad0eca0))


### Bug Fixes

* restore classic window layering and dragging ([#823](https://github.com/benletchford/systemless/issues/823)) ([6fb2508](https://github.com/benletchford/systemless/commit/6fb250840d948501cdd6c046a89d0856e819dc16))

## [0.18.25](https://github.com/benletchford/systemless/compare/v0.18.24...v0.18.25) (2026-08-25)


### Bug Fixes

* repeat held scrollbar actions ([#817](https://github.com/benletchford/systemless/issues/817)) ([02052b4](https://github.com/benletchford/systemless/commit/02052b48343283620491d8d90ce7f384ade808ff))

## [0.18.24](https://github.com/benletchford/systemless/compare/v0.18.23...v0.18.24) (2026-08-25)


### Bug Fixes

* support authentic multi-disk game launches ([#815](https://github.com/benletchford/systemless/issues/815)) ([2ccff10](https://github.com/benletchford/systemless/commit/2ccff101239277b8427011d1a217bfd901dc7d7d))

## [0.18.23](https://github.com/benletchford/systemless/compare/v0.18.22...v0.18.23) (2026-08-25)


### Bug Fixes

* restore PowerPC cursor and menu behavior ([eeddde1](https://github.com/benletchford/systemless/commit/eeddde1bfe3a1b28f30f4b16bf89b1e816ac15fd))

## [0.18.22](https://github.com/benletchford/systemless/compare/v0.18.21...v0.18.22) (2026-08-25)


### Bug Fixes

* **ppc:** align system menu mark behavior ([#809](https://github.com/benletchford/systemless/issues/809)) ([0db4592](https://github.com/benletchford/systemless/commit/0db4592ce7f85262bdcbbd5bad940a489dc7019c))

## [0.18.21](https://github.com/benletchford/systemless/compare/v0.18.20...v0.18.21) (2026-08-25)


### Bug Fixes

* **ppc:** preserve keyed Quilt sprite backgrounds ([#807](https://github.com/benletchford/systemless/issues/807)) ([3c705ed](https://github.com/benletchford/systemless/commit/3c705ed7ba3f2b09700c9a11c9356aa181008ea4))

## [0.18.20](https://github.com/benletchford/systemless/compare/v0.18.19...v0.18.20) (2026-08-25)


### Bug Fixes

* **ppc:** restore Gridz QuickDraw rendering ([54b5f69](https://github.com/benletchford/systemless/commit/54b5f69bfc66fe91f3b3dea884190edcc7e083f7)), closes [#801](https://github.com/benletchford/systemless/issues/801)

## [0.18.19](https://github.com/benletchford/systemless/compare/v0.18.18...v0.18.19) (2026-08-24)


### Bug Fixes

* **game:** honor explicit PowerPC executable preference ([#799](https://github.com/benletchford/systemless/issues/799)) ([837f5ea](https://github.com/benletchford/systemless/commit/837f5ea9ba774a662b7bf320f02f9db32ea12de7))

## [0.18.18](https://github.com/benletchford/systemless/compare/v0.18.17...v0.18.18) (2026-08-24)


### Bug Fixes

* **file:** preserve closed paths across PBGetFInfo ([#796](https://github.com/benletchford/systemless/issues/796)) ([80f9e15](https://github.com/benletchford/systemless/commit/80f9e15f13b6d8a9627face4d4b74675c9c45b44))
* **ppc:** exclude scanline padding from presented frames ([#795](https://github.com/benletchford/systemless/issues/795)) ([2a80efb](https://github.com/benletchford/systemless/commit/2a80efb50ce49d868a616f6ee02bd18fc041b1e5))
* **quickdraw:** align main and offscreen scanline storage ([#794](https://github.com/benletchford/systemless/issues/794)) ([086f388](https://github.com/benletchford/systemless/commit/086f388d3eaf3a6b279afda6e9ca0f728d90dbd9))

## [0.18.17](https://github.com/benletchford/systemless/compare/v0.18.16...v0.18.17) (2026-08-24)


### Bug Fixes

* extract grouped VISE resources ([#788](https://github.com/benletchford/systemless/issues/788)) ([163ad9b](https://github.com/benletchford/systemless/commit/163ad9b37976217d5510b8cb76b33a32a2483d8a))

## [0.18.16](https://github.com/benletchford/systemless/compare/v0.18.15...v0.18.16) (2026-08-23)


### Bug Fixes

* launch installed PowerPC games reliably ([#785](https://github.com/benletchford/systemless/issues/785)) ([961a2ce](https://github.com/benletchford/systemless/commit/961a2ce394d9f9dd9ed9e3e4b9f725e9d7bb0618))

## [0.18.15](https://github.com/benletchford/systemless/compare/v0.18.14...v0.18.15) (2026-08-23)


### Bug Fixes

* **resource:** materialize Quilt qDir resources and decode raw frames ([#779](https://github.com/benletchford/systemless/issues/779)) ([68dd343](https://github.com/benletchford/systemless/commit/68dd343b0f74602c785104085db052a4ce22c9f7))

## [0.18.14](https://github.com/benletchford/systemless/compare/v0.18.13...v0.18.14) (2026-08-23)


### Bug Fixes

* complete native PowerPC CFM runtime support ([#774](https://github.com/benletchford/systemless/issues/774)) ([fe37fd5](https://github.com/benletchford/systemless/commit/fe37fd5ee64e14bec20b3faa2d2bb01b2a707711))

## [0.18.13](https://github.com/benletchford/systemless/compare/v0.18.12...v0.18.13) (2026-08-23)


### Bug Fixes

* complete MINDit PowerPC runtime support ([#756](https://github.com/benletchford/systemless/issues/756)) ([a4b7737](https://github.com/benletchford/systemless/commit/a4b77379b8ddc537e94b5938c20999e113ce3382))
* complete Spins native PowerPC compatibility ([#758](https://github.com/benletchford/systemless/issues/758)) ([87c2d9e](https://github.com/benletchford/systemless/commit/87c2d9e1f0e4bd22e89b38f4ebe302cc1e248e98))
* preserve modal alerts and absent screen saver state ([#768](https://github.com/benletchford/systemless/issues/768)) ([5c18232](https://github.com/benletchford/systemless/commit/5c182328638feb5b5574ec62a9a47ae12cf44004))
* **quickdraw:** support enhanced standard color tables ([#771](https://github.com/benletchford/systemless/issues/771)) ([d6e7ba8](https://github.com/benletchford/systemless/commit/d6e7ba8a93eec5cb7a68191684e83f1858f95160))
* support international resources and fair interrupt timing ([#765](https://github.com/benletchford/systemless/issues/765)) ([7383493](https://github.com/benletchford/systemless/commit/73834935cc048dabdbe913a675723a9f2888de67))

## [0.18.12](https://github.com/benletchford/systemless/compare/v0.18.11...v0.18.12) (2026-08-22)


### Bug Fixes

* return 0x0100 for pascal boolean true in testdeviceattribute ([#762](https://github.com/benletchford/systemless/issues/762)) ([54b6b16](https://github.com/benletchford/systemless/commit/54b6b16e28688a2c67dbee9bf5dc1231ba986ce3))

## [0.18.11](https://github.com/benletchford/systemless/compare/v0.18.10...v0.18.11) (2026-08-22)


### Bug Fixes

* complete Capitalism native PowerPC compatibility ([#737](https://github.com/benletchford/systemless/issues/737)) ([156639a](https://github.com/benletchford/systemless/commit/156639ab92076bbac8089d8f6888b8c0e2d3c09d))
* **quickdraw:** resolve clip port fallbacks and prevent TEUpdate background erasure ([#760](https://github.com/benletchford/systemless/issues/760)) ([da0aacf](https://github.com/benletchford/systemless/commit/da0aacf3ee4de6454dbc66294bb7759a8b171c6b))
* run Blue's ABC Time Activities ([#748](https://github.com/benletchford/systemless/issues/748)) ([0158eb3](https://github.com/benletchford/systemless/commit/0158eb33afcb7f9316e35220c33c671d30af471c))
* support Battle Girl PowerPC compatibility ([#753](https://github.com/benletchford/systemless/issues/753)) ([c31d1ed](https://github.com/benletchford/systemless/commit/c31d1ed72d187c68105e8a709ee0e363263d2e87))

## [0.18.10](https://github.com/benletchford/systemless/compare/v0.18.9...v0.18.10) (2026-08-22)


### Bug Fixes

* restore classic key map bit ordering ([#749](https://github.com/benletchford/systemless/issues/749)) ([73e30c0](https://github.com/benletchford/systemless/commit/73e30c0d6022f0b1f059f239b089937e09033d20))

## [0.18.9](https://github.com/benletchford/systemless/compare/v0.18.8...v0.18.9) (2026-08-22)


### Bug Fixes

* **menu:** return zero from PopUpMenuSelect when mouse button is up ([#744](https://github.com/benletchford/systemless/issues/744)) ([c0b96db](https://github.com/benletchford/systemless/commit/c0b96db8fa60fd6365126b3a5a9dda701d759334))

## [0.18.8](https://github.com/benletchford/systemless/compare/v0.18.7...v0.18.8) (2026-08-22)


### Bug Fixes

* complete Dynasty League Baseball compatibility ([#733](https://github.com/benletchford/systemless/issues/733)) ([9da653e](https://github.com/benletchford/systemless/commit/9da653e4642b4ab6e0f49c5be734f5fd5e7554d5))
* resolve dialog picture items against logical palette during hardware fades ([#740](https://github.com/benletchford/systemless/issues/740)) ([0e6a446](https://github.com/benletchford/systemless/commit/0e6a4463175f6fc5721caca5eabddcd375c45f70))

## [0.18.7](https://github.com/benletchford/systemless/compare/v0.18.6...v0.18.7) (2026-08-21)


### Bug Fixes

* complete PowerMonger compatibility ([#720](https://github.com/benletchford/systemless/issues/720)) ([8d0716a](https://github.com/benletchford/systemless/commit/8d0716ae7eacb014bfbd06239585bb5c7b9b7e9f))
* render Operation Crusader menu items ([#722](https://github.com/benletchford/systemless/issues/722)) ([8f5c30b](https://github.com/benletchford/systemless/commit/8f5c30bcb0e58692d7f099f6b891270a1d8dbe71))

## [0.18.6](https://github.com/benletchford/systemless/compare/v0.18.5...v0.18.6) (2026-08-20)


### Bug Fixes

* **input:** separate horizontal and vertical axes ([#714](https://github.com/benletchford/systemless/issues/714)) ([7c2d8a6](https://github.com/benletchford/systemless/commit/7c2d8a61bfa8a6fc7d2b927c7fb2f91d60946549))
* **sound:** allow longer PowerPC doubleback callbacks ([#716](https://github.com/benletchford/systemless/issues/716)) ([303f1b1](https://github.com/benletchford/systemless/commit/303f1b1ecfae36c82705629f64670f6b63e8b25f))

## [0.18.5](https://github.com/benletchford/systemless/compare/v0.18.4...v0.18.5) (2026-08-20)


### Bug Fixes

* **desktop:** drive guest progress for mouse input ([#712](https://github.com/benletchford/systemless/issues/712)) ([3c87f8a](https://github.com/benletchford/systemless/commit/3c87f8a73dd1254f10b525c3b7d0d0144c149579))
* **desktop:** release clicks after guest execution ([#709](https://github.com/benletchford/systemless/issues/709)) ([6ca70cf](https://github.com/benletchford/systemless/commit/6ca70cf033d687ed46b42a4c083431acabb2580b))
* preserve PPC display palettes and front buffers ([#707](https://github.com/benletchford/systemless/issues/707)) ([cec8047](https://github.com/benletchford/systemless/commit/cec8047e6287304886f04a5a2379a564819fba51))

## [0.18.4](https://github.com/benletchford/systemless/compare/v0.18.3...v0.18.4) (2026-08-20)


### Bug Fixes

* **desktop:** preserve short mouse clicks ([deee36f](https://github.com/benletchford/systemless/commit/deee36f27a6347eb6e3aa10721010a61c9b46e94))
* **pict:** preserve exact indexed colors ([#705](https://github.com/benletchford/systemless/issues/705)) ([5e50397](https://github.com/benletchford/systemless/commit/5e503972ad8db4d3cd0637e9326563a6bb7171d3))

## [0.18.3](https://github.com/benletchford/systemless/compare/v0.18.2...v0.18.3) (2026-08-20)


### Bug Fixes

* **input:** return PowerPC mouse positions in local coordinates ([#700](https://github.com/benletchford/systemless/issues/700)) ([0c6c535](https://github.com/benletchford/systemless/commit/0c6c53585986e2d8e7656410fa91a9c557f0e6cc))

## [0.18.2](https://github.com/benletchford/systemless/compare/v0.18.1...v0.18.2) (2026-08-20)


### Bug Fixes

* launch extended VISE PowerPC games ([4443f1c](https://github.com/benletchford/systemless/commit/4443f1c2e6d83cdc2bace789ecdcf980662c202b))
* **palette:** install explicit-tolerant palette colors ([25e6aeb](https://github.com/benletchford/systemless/commit/25e6aebc93502fa23907b47c7a24477084cadd57)), closes [#695](https://github.com/benletchford/systemless/issues/695)
* **quickdraw:** scale text width with the requested size ([dcc9d6d](https://github.com/benletchford/systemless/commit/dcc9d6d36cc7f1da595857d3b0aab0033f76ab16))
* **resource:** load lazily seeded resources on demand ([e48963c](https://github.com/benletchford/systemless/commit/e48963c4ab5caa2d0d8b0397c4dc3db13131a43a)), closes [#692](https://github.com/benletchford/systemless/issues/692)


### Performance Improvements

* decompress StuffIt forks on every core ([1546406](https://github.com/benletchford/systemless/commit/15464063d1c3039058a066f6baad5fe849408db3))
* **framebuffer:** tile the desktop pattern fill by row on 8-bit screens ([a26ed7c](https://github.com/benletchford/systemless/commit/a26ed7c01a98d15e697c21db55e26e497e5f8c2b))
* **memory:** scribble uninitialized allocations with one bulk fill ([b3bf752](https://github.com/benletchford/systemless/commit/b3bf752fdc9a6c68962c0b598bb15b6ef4492110))
* **pict:** blit region-masked 8-bit PICT rows as spans ([7497bfc](https://github.com/benletchford/systemless/commit/7497bfc37a4f51250b3da2057462566b4a6ffdde))
* **pict:** draw 1-bit srcCopy source rows through the 8-bit row path ([caffe97](https://github.com/benletchford/systemless/commit/caffe979b6fb786d66273974b6f6af7f03741889))
* **runner:** budget idle-cycle probes per site and tick; cap the journal ([5d29448](https://github.com/benletchford/systemless/commit/5d2944851ed98ee02b9cf6434ce61036607036b3))
* **shapes:** fill, erase and invert rectangles a row at a time on 1-bit and 8-bit ports ([9453af4](https://github.com/benletchford/systemless/commit/9453af41ca0fb815ac955edd5638f4959bf4571a))
* **text:** stop re-scanning the colour table per glyph; iterate only the glyph box ([4a09a78](https://github.com/benletchford/systemless/commit/4a09a78df6fb3326732108d9c9409ecb7150293e))

## [0.18.1](https://github.com/benletchford/systemless/compare/v0.18.0...v0.18.1) (2026-08-19)


### Bug Fixes

* **sound:** load disk-backed sound resources on demand ([#685](https://github.com/benletchford/systemless/issues/685)) ([bd45aa8](https://github.com/benletchford/systemless/commit/bd45aa8a1b4466f7431ea8d66d515a3199784504))

## [0.18.0](https://github.com/benletchford/systemless/compare/v0.17.1...v0.18.0) (2026-08-19)


### Features

* allow builds to use the portable 68K executor ([#682](https://github.com/benletchford/systemless/issues/682)) ([186112f](https://github.com/benletchford/systemless/commit/186112f2972465a86c0aabf9be77cfa8702368df))

## [0.17.1](https://github.com/benletchford/systemless/compare/v0.17.0...v0.17.1) (2026-08-19)


### Bug Fixes

* **quickdraw:** reload custom color tables ([59a8970](https://github.com/benletchford/systemless/commit/59a89704bf8b8bfd5dd1de9ed8eea26093db89a0))

## [0.17.0](https://github.com/benletchford/systemless/compare/v0.16.1...v0.17.0) (2026-08-18)


### Features

* **cli:** add --prefer-ppc alias ([59b01b5](https://github.com/benletchford/systemless/commit/59b01b508c2ec6f201f27670fed38d2397dcad8f)), closes [#670](https://github.com/benletchford/systemless/issues/670)

## [0.16.1](https://github.com/benletchford/systemless/compare/v0.16.0...v0.16.1) (2026-08-17)


### Bug Fixes

* launch large universal applications ([650536c](https://github.com/benletchford/systemless/commit/650536cd7ecd4938da3622f369f4420a055ebf6c)), closes [#658](https://github.com/benletchford/systemless/issues/658) [#659](https://github.com/benletchford/systemless/issues/659)

## [0.16.0](https://github.com/benletchford/systemless/compare/v0.15.0...v0.16.0) (2026-08-17)


### Features

* **display:** support configurable 4-bit mode ([#568](https://github.com/benletchford/systemless/issues/568)) ([b6608f2](https://github.com/benletchford/systemless/commit/b6608f2c71e24ec539ee9179ad2eff1429835782))
* **display:** support selectable one-bit mode ([#594](https://github.com/benletchford/systemless/issues/594)) ([75e4311](https://github.com/benletchford/systemless/commit/75e431103c8d33a82862e52f269912e34480dfed))
* **files:** expose extracted read-only volumes ([#536](https://github.com/benletchford/systemless/issues/536)) ([4922f8b](https://github.com/benletchford/systemless/commit/4922f8bdb5c087d37be9577b257ca96af30bd6ac))
* **files:** implement read-only catalog search ([#548](https://github.com/benletchford/systemless/issues/548)) ([d374de8](https://github.com/benletchford/systemless/commit/d374de8309364957a5df907f040cde3e1957e648))
* load multi-file Macintosh ZIP game archives ([#551](https://github.com/benletchford/systemless/issues/551)) ([4756d72](https://github.com/benletchford/systemless/commit/4756d72adaffe22b31fb1cc297113c98714b8f6e))
* **memory:** support 24-bit address translation ([#571](https://github.com/benletchford/systemless/issues/571)) ([b75135a](https://github.com/benletchford/systemless/commit/b75135a700eb6c359126754d1557b77a0182fe44))


### Bug Fixes

* clip fully occluded background dialogs ([#609](https://github.com/benletchford/systemless/issues/609)) ([a15c7c5](https://github.com/benletchford/systemless/commit/a15c7c525e2882e0b5b252d1ab2282024506b6be))
* coalesce window activation events through classic pending slots ([#599](https://github.com/benletchford/systemless/issues/599)) ([c54a962](https://github.com/benletchford/systemless/commit/c54a962d792dba3b5f80cc981bf8b44bcd5b3622))
* **control:** preserve disabled popup titles ([#506](https://github.com/benletchford/systemless/issues/506)) ([27e0924](https://github.com/benletchford/systemless/commit/27e0924466f3e6b0826c73db8e39f2b3db7d4b92))
* decode unpacked indexed PICT pixmaps ([#607](https://github.com/benletchford/systemless/issues/607)) ([0102d56](https://github.com/benletchford/systemless/commit/0102d567744efc9dfe5d7d11df3cfb44c51bc0ba))
* **dialog:** apply matching dialog color tables ([#520](https://github.com/benletchford/systemless/issues/520)) ([f41d1f3](https://github.com/benletchford/systemless/commit/f41d1f3e64dfb2cb5197f77b1b7b12a88223d0f7))
* **dialog:** leave Command-A to applications ([#512](https://github.com/benletchford/systemless/issues/512)) ([fd88719](https://github.com/benletchford/systemless/commit/fd887198a18a006cf34e8f751d93ed3046af9da4))
* **dialog:** preserve premodal application drawing ([#527](https://github.com/benletchford/systemless/issues/527)) ([74c78e1](https://github.com/benletchford/systemless/commit/74c78e15c9d25c99d54a3103a5372c6992fc41a0))
* **dialog:** preserve standard dialog control state ([#521](https://github.com/benletchford/systemless/issues/521)) ([8719fb7](https://github.com/benletchford/systemless/commit/8719fb74c28f6684474fb5933d340cc010c0641c))
* **dialog:** preserve user-item GrafPort state ([#573](https://github.com/benletchford/systemless/issues/573)) ([197a647](https://github.com/benletchford/systemless/commit/197a647ceb63d8b994055c4e527487e890b99414))
* **dialog:** protect retained save-under pixels ([#510](https://github.com/benletchford/systemless/issues/510)) ([0426f03](https://github.com/benletchford/systemless/commit/0426f0386b132f644091a45e65ff98e29e29b39e))
* **dialog:** release buttons before nested modals ([#525](https://github.com/benletchford/systemless/issues/525)) ([5c06acd](https://github.com/benletchford/systemless/commit/5c06acdf1afbecf054f7efeca2e1fed2c36db04c))
* **dialog:** retain filtered alerts ([#588](https://github.com/benletchford/systemless/issues/588)) ([fd38330](https://github.com/benletchford/systemless/commit/fd3833057e1e505285dfc761ba6d139ea6259db4))
* **dialog:** translate color icon palettes ([#526](https://github.com/benletchford/systemless/issues/526)) ([7f6152f](https://github.com/benletchford/systemless/commit/7f6152f2439c1381880c8a7268611d283432a156))
* **display:** preserve direct driver palette values ([#558](https://github.com/benletchford/systemless/issues/558)) ([0c0a615](https://github.com/benletchford/systemless/commit/0c0a6154d84c5733f301c356abf7be0d6ada0207))
* fence disposed modal dialog presses ([#508](https://github.com/benletchford/systemless/issues/508)) ([66b1dea](https://github.com/benletchford/systemless/commit/66b1dea8fee1454215d6151a8f260a3587745165))
* honor small classic application partitions ([#605](https://github.com/benletchford/systemless/issues/605)) ([33db22a](https://github.com/benletchford/systemless/commit/33db22a466d13b52e6953422a74674a160b7cec2))
* **list:** preserve offset cell rendering ([#519](https://github.com/benletchford/systemless/issues/519)) ([cebc186](https://github.com/benletchford/systemless/commit/cebc186c7d62807e763f182bbfe646641b7942d3))
* load runtime CODE resources on segment faults ([#593](https://github.com/benletchford/systemless/issues/593)) ([af347ca](https://github.com/benletchford/systemless/commit/af347ca3e38f00a87bf6b47e41261b75b7e034ce))
* match indexed PICT colors through device inverse table ([#533](https://github.com/benletchford/systemless/issues/533)) ([b9239cd](https://github.com/benletchford/systemless/commit/b9239cd55c21262807fcdb655f80740ee764953f))
* **memory:** reserve callback code below framebuffer ([#626](https://github.com/benletchford/systemless/issues/626)) ([323ecee](https://github.com/benletchford/systemless/commit/323ecee9d8ff359c70de64a74e5efc2b91021cdd))
* **menu:** preserve long menu records ([#514](https://github.com/benletchford/systemless/issues/514)) ([1cabca1](https://github.com/benletchford/systemless/commit/1cabca167dfdb87304b13d97d336ec07ae9356cd))
* **menu:** preserve themed highlighted content ([#532](https://github.com/benletchford/systemless/issues/532)) ([754c1c3](https://github.com/benletchford/systemless/commit/754c1c3ab43c4fc9574aecd343d97570cf236450))
* **menu:** search installed command-only menus ([#511](https://github.com/benletchford/systemless/issues/511)) ([03ff656](https://github.com/benletchford/systemless/commit/03ff656472ff5cb74db2855be769e350ae654dfa))
* **palette:** preserve indexed window colors ([#553](https://github.com/benletchford/systemless/issues/553)) ([cb29de6](https://github.com/benletchford/systemless/commit/cb29de64a4d6a438c6ea44046e32165f018ae0eb))
* preserve device colors for explicit palette entries ([#610](https://github.com/benletchford/systemless/issues/610)) ([6ea9fb4](https://github.com/benletchford/systemless/commit/6ea9fb47029ce498aa96ac6be63495f055bbdce9))
* preserve explicit colors across client-ID palette installs ([#509](https://github.com/benletchford/systemless/issues/509)) ([2fa865a](https://github.com/benletchford/systemless/commit/2fa865ab9dfde3e277f3bf4f97f05294242c5463))
* preserve indexed Boolean CopyBits semantics ([#601](https://github.com/benletchford/systemless/issues/601)) ([98acaf0](https://github.com/benletchford/systemless/commit/98acaf09cd43a67bc12a6e7dd5a5f3b9ee2bf801))
* preserve nonvolatile registers across native LoadSeg handoff ([#603](https://github.com/benletchford/systemless/issues/603)) ([de24fda](https://github.com/benletchford/systemless/commit/de24fda92a4cb8216e681978eca3c43aba9dd8d4))
* preserve resource references and map enumeration order ([#596](https://github.com/benletchford/systemless/issues/596)) ([f027e42](https://github.com/benletchford/systemless/commit/f027e42c88700ecea13ebbe49b9fe904b1064fed))
* **resource:** expose application resource map ([#587](https://github.com/benletchford/systemless/issues/587)) ([53c01e2](https://github.com/benletchford/systemless/commit/53c01e2684ded34e1f393d77cc5df48218dc0e8c))
* **runner:** advance callback-gated headless audio ([#535](https://github.com/benletchford/systemless/issues/535)) ([9c34379](https://github.com/benletchford/systemless/commit/9c3437943995dde29aeecd93c1df344b81d25163))
* **runner:** pace Standard File refires ([#518](https://github.com/benletchford/systemless/issues/518)) ([fc2c936](https://github.com/benletchford/systemless/commit/fc2c9367fa89743cdda0f18ba6face23e0d91112))
* **script:** implement script-aware ReplaceText ([#537](https://github.com/benletchford/systemless/issues/537)) ([a447789](https://github.com/benletchford/systemless/commit/a447789aae3c386bfbd2762a8c5c9ba642c978ae))
* **sound:** consume Director dispatch argument ([#586](https://github.com/benletchford/systemless/issues/586)) ([efa23da](https://github.com/benletchford/systemless/commit/efa23da2c2a4235c19700f70dc42fe4336f8d8fd))
* **sound:** dispatch legacy Sound Driver writes ([#505](https://github.com/benletchford/systemless/issues/505)) ([4a1a069](https://github.com/benletchford/systemless/commit/4a1a069b5fa3ab36aede895b94317937ba61c8e3))
* **standard-file:** restore classic Open dialog ([#574](https://github.com/benletchford/systemless/issues/574)) ([88e769d](https://github.com/benletchford/systemless/commit/88e769d7924e66f36691c467993a32068c343941))
* support legacy Launch parameter records ([#590](https://github.com/benletchford/systemless/issues/590)) ([7dd7b58](https://github.com/benletchford/systemless/commit/7dd7b582d948fbf9f8782536b9bbf207e724faf6))
* **textedit:** render delayed selected styles ([#554](https://github.com/benletchford/systemless/issues/554)) ([f99b987](https://github.com/benletchford/systemless/commit/f99b9870d1d93282bcd6e045f8fe600290c14378))
* use 16-bit midpoint for monochrome CopyBits ([#570](https://github.com/benletchford/systemless/issues/570)) ([4558b0c](https://github.com/benletchford/systemless/commit/4558b0ca6539c868b6785449ba1862c3c7af5dd0))
* **window:** apply WIND positioning specifications ([#523](https://github.com/benletchford/systemless/issues/523)) ([9dd84ae](https://github.com/benletchford/systemless/commit/9dd84aee86261ebe8bf212b2bea86809c4e22ec2))
* **window:** draw active movable dialog chrome ([#524](https://github.com/benletchford/systemless/issues/524)) ([4c78811](https://github.com/benletchford/systemless/commit/4c788110d19a419b8a42b54ab7d17e0b478d8be3))
* **window:** repaint content exposed by window changes ([#560](https://github.com/benletchford/systemless/issues/560)) ([4a01362](https://github.com/benletchford/systemless/commit/4a013624a8cfb29b1f294dc964a4388dfdd4b789))
* **window:** repaint exposed content after moves ([#522](https://github.com/benletchford/systemless/issues/522)) ([b0830c1](https://github.com/benletchford/systemless/commit/b0830c1cd010c7c477afbf5b55a1809d28745397))


### Performance Improvements

* prove bounded journal-complete poll cycles ([#644](https://github.com/benletchford/systemless/issues/644)) ([6425f35](https://github.com/benletchford/systemless/commit/6425f3512a37e4f5e80639006c4b97785134e274))

## [0.15.0](https://github.com/benletchford/systemless/compare/v0.14.1...v0.15.0) (2026-08-17)


### Features

* add a flag to disable native integrations ([#654](https://github.com/benletchford/systemless/issues/654)) ([1947cf4](https://github.com/benletchford/systemless/commit/1947cf47f384e89a0320438dc4f9ec1efd31f493))
* show guest application names in the macOS Dock ([#652](https://github.com/benletchford/systemless/issues/652)) ([2ae6541](https://github.com/benletchford/systemless/commit/2ae6541188406996502386538866733344068cfb))

## [0.14.1](https://github.com/benletchford/systemless/compare/v0.14.0...v0.14.1) (2026-08-17)


### Bug Fixes

* support HyperCard fonts, menus, and window scaling ([#649](https://github.com/benletchford/systemless/issues/649)) ([3ab7c05](https://github.com/benletchford/systemless/commit/3ab7c05bf759cfca657d6569edf28fad69105886))

## [0.14.0](https://github.com/benletchford/systemless/compare/v0.13.3...v0.14.0) (2026-08-17)


### Features

* expose GPU-ready QD3D frames and fast-forward PowerPC idle polls ([#646](https://github.com/benletchford/systemless/issues/646)) ([7aba7da](https://github.com/benletchford/systemless/commit/7aba7da5fc11709caa77627ff43ea55878575194))

## [0.13.3](https://github.com/benletchford/systemless/compare/v0.13.2...v0.13.3) (2026-08-16)


### Performance Improvements

* accelerate QD3D triangle rasterization ([767279e](https://github.com/benletchford/systemless/commit/767279e9ef6795f26c38bf371f671002ec1bdde6))

## [0.13.2](https://github.com/benletchford/systemless/compare/v0.13.1...v0.13.2) (2026-08-16)


### Performance Improvements

* adopt cached PowerPC basic blocks ([#640](https://github.com/benletchford/systemless/issues/640)) ([c71f9f3](https://github.com/benletchford/systemless/commit/c71f9f3026a2372be6107255b77ffe6f228cae34))
* reuse PowerPC HLE state between execution slices ([#637](https://github.com/benletchford/systemless/issues/637)) ([d07da75](https://github.com/benletchford/systemless/commit/d07da75ae15e88ffc0f1444364264f6ffdeb7769))

## [0.13.1](https://github.com/benletchford/systemless/compare/v0.13.0...v0.13.1) (2026-08-16)


### Performance Improvements

* defer PowerPC host synchronization between GUI slices ([c145d81](https://github.com/benletchford/systemless/commit/c145d8160451e6f86baae50b2a283abab8696891))

## [0.13.0](https://github.com/benletchford/systemless/compare/v0.12.17...v0.13.0) (2026-08-16)


### Features

* **ppc:** add initial PowerPC application support ([#631](https://github.com/benletchford/systemless/issues/631)) ([ec5c263](https://github.com/benletchford/systemless/commit/ec5c263509ba8e689ce09a4709f8995ddbb0f367))


### Performance Improvements

* **fonts:** resolve the override directory once, not per glyph ([#628](https://github.com/benletchford/systemless/issues/628)) ([4158bd8](https://github.com/benletchford/systemless/commit/4158bd8f5fe4134940410e95d27a63c0011dd0be))

## [0.12.17](https://github.com/benletchford/systemless/compare/v0.12.16...v0.12.17) (2026-08-14)


### Bug Fixes

* **control:** draw controls when shown ([687702a](https://github.com/benletchford/systemless/commit/687702a2c8d9c95a9e8377bfb0566dc51f1a036c))
* **device:** hide unavailable AppleTalk drivers ([de8c9c6](https://github.com/benletchford/systemless/commit/de8c9c663c0370444c86876532f0802f8611b7ea))
* drain modal dialog draw callbacks before resuming events ([084a5d0](https://github.com/benletchford/systemless/commit/084a5d08fb6e88cbcfa30a45a12300a79e4b7dc7))
* prefer exact executable override matches ([93a821f](https://github.com/benletchford/systemless/commit/93a821f00050700ade484db8854f829a2736d8f7))
* **quickdraw:** convert direct pixels to indexed color ([e28f4c9](https://github.com/benletchford/systemless/commit/e28f4c98bd8cd1a7f6d711390bf68952cf35fcaa))
* **sane:** implement FDEC2STR formatting ([3699b12](https://github.com/benletchford/systemless/commit/3699b1220643bd21b473f64d0ddb89ff5f47be68))

## [0.12.16](https://github.com/benletchford/systemless/compare/v0.12.15...v0.12.16) (2026-08-13)


### Bug Fixes

* **menu:** hide MenuKey title highlights with menu bar ([f729115](https://github.com/benletchford/systemless/commit/f729115ec820e4349312acca5ce30270abc7f9cd))

## [0.12.15](https://github.com/benletchford/systemless/compare/v0.12.14...v0.12.15) (2026-08-13)


### Bug Fixes

* restore formatting and Clippy release gates ([da32ef9](https://github.com/benletchford/systemless/commit/da32ef9213a15553e4283f2b2733e47c42aa0a09))
* **window:** exit after clean guest quit ([c4a271c](https://github.com/benletchford/systemless/commit/c4a271c05c3bc840b247db186af6f5d6ac29196f))

## [0.12.14](https://github.com/benletchford/systemless/compare/v0.12.13...v0.12.14) (2026-08-12)


### Bug Fixes

* **control:** truncate fixed-width popup titles ([99176f2](https://github.com/benletchford/systemless/commit/99176f2cfa6529b4ba133aa715b40bb6e7509723))

## [0.12.13](https://github.com/benletchford/systemless/compare/v0.12.12...v0.12.13) (2026-08-11)


### Bug Fixes

* **input:** latch Caps Lock state ([610d8ed](https://github.com/benletchford/systemless/commit/610d8ed1c5355a71d54a90a7bc9a467ebb673b8f))

## [0.12.12](https://github.com/benletchford/systemless/compare/v0.12.11...v0.12.12) (2026-08-10)


### Bug Fixes

* position parent-relative dialog frames over the front window ([13f99e8](https://github.com/benletchford/systemless/commit/13f99e8625c8d8d8955f37ab0dd252579e05456c))

## [0.12.11](https://github.com/benletchford/systemless/compare/v0.12.10...v0.12.11) (2026-08-10)


### Performance Improvements

* direct-index the native-trap table ([7bde41f](https://github.com/benletchford/systemless/commit/7bde41f32a63af7fd98f4232ffd7bd4d07c4e057))
* stop cloning the window list on every event poll ([fd6e80c](https://github.com/benletchford/systemless/commit/fd6e80cb2b1f507009e2cf5e1987684eded4373b))

## [0.12.10](https://github.com/benletchford/systemless/compare/v0.12.9...v0.12.10) (2026-08-09)


### Performance Improvements

* keep the application-launch path out of the hot loop ([cc354b7](https://github.com/benletchford/systemless/commit/cc354b7d196635e5cbd31227f94e21ad767925a1))

## [0.12.9](https://github.com/benletchford/systemless/compare/v0.12.8...v0.12.9) (2026-08-09)


### Performance Improvements

* bound the read-only code check on the guest write path ([e8712d8](https://github.com/benletchford/systemless/commit/e8712d81e04c7ca041c76bcd9f8f48ed2fdc293c))

## [0.12.8](https://github.com/benletchford/systemless/compare/v0.12.7...v0.12.8) (2026-08-07)


### Bug Fixes

* preserve black kiosk margins around transient windows ([10b616c](https://github.com/benletchford/systemless/commit/10b616c0e16385aaeb83c3b1aaf428e58029abf9))

## [0.12.7](https://github.com/benletchford/systemless/compare/v0.12.6...v0.12.7) (2026-08-06)


### Bug Fixes

* clear stale list selections after blank clicks ([ee99175](https://github.com/benletchford/systemless/commit/ee99175b1e1da6372f75c1f4a56c27dfffc5233d))

## [0.12.6](https://github.com/benletchford/systemless/compare/v0.12.5...v0.12.6) (2026-08-06)


### Bug Fixes

* render menu command symbols with built-in fonts ([c07ccae](https://github.com/benletchford/systemless/commit/c07ccae59d2cd05f60852ee2e0d249be7ee1898a))

## [0.12.5](https://github.com/benletchford/systemless/compare/v0.12.4...v0.12.5) (2026-08-06)


### Bug Fixes

* defer modal dialog snapshots until filter callbacks finish ([6e61aa3](https://github.com/benletchford/systemless/commit/6e61aa3fbb6ae80f6696f98333fd4d109232646d))

## [0.12.4](https://github.com/benletchford/systemless/compare/v0.12.3...v0.12.4) (2026-08-06)


### Bug Fixes

* preserve painted margins around centered blits ([09fccda](https://github.com/benletchford/systemless/commit/09fccdafc376254acf29cf19acab7eb4db88f889))

## [0.12.3](https://github.com/benletchford/systemless/compare/v0.12.2...v0.12.3) (2026-08-06)


### Bug Fixes

* preserve menu marks and command-key equivalents ([e0f133c](https://github.com/benletchford/systemless/commit/e0f133c754b1f22f9335bea64257b8e54edde747))

## [0.12.2](https://github.com/benletchford/systemless/compare/v0.12.1...v0.12.2) (2026-08-05)


### Bug Fixes

* redraw List Manager scrollbars during updates ([9c9c28b](https://github.com/benletchford/systemless/commit/9c9c28bcebdda855ec40b214a8bfc127ef5a4997))

## [0.12.1](https://github.com/benletchford/systemless/compare/v0.12.0...v0.12.1) (2026-08-05)


### Build System

* update m68k to 0.7.1 ([#406](https://github.com/benletchford/systemless/issues/406)) ([63b69b7](https://github.com/benletchford/systemless/commit/63b69b7fb04a44aa7a19c79390fa38ce5ba7e9c8))

## [0.12.0](https://github.com/benletchford/systemless/compare/v0.11.5...v0.12.0) (2026-08-05)


### Features

* extract HFS volumes from Apple Partition Map images ([55b5319](https://github.com/benletchford/systemless/commit/55b5319ac4100aca2410d006badf8444b1542097))


### Bug Fixes

* blend inactive control titles through the live palette ([#387](https://github.com/benletchford/systemless/issues/387)) ([eecd7a4](https://github.com/benletchford/systemless/commit/eecd7a44506c3cae4b9b00380f659fd4d6145d39))
* center-sample scaled indexed PICT pixels ([eed018f](https://github.com/benletchford/systemless/commit/eed018fb914933eef8a99146e5ac6f87628803af))
* classify legacy GetTrapAddress targets by trap number ([3f2a6cc](https://github.com/benletchford/systemless/commit/3f2a6ccecb2819fc3fc212f8932c73549a2800fa))
* compute fractional SANE powers ([#370](https://github.com/benletchford/systemless/issues/370)) ([fcbb3e3](https://github.com/benletchford/systemless/commit/fcbb3e37bb9a6bc8c23ee4dcbfaaea36f1136382))
* create List Manager scrollbars from the documented LNew frame ([#391](https://github.com/benletchford/systemless/issues/391)) ([3c1a302](https://github.com/benletchford/systemless/commit/3c1a302f825c3df66a45282e649c59122c3b169e))
* defer tracking refire until asynchronous callbacks return ([b703a88](https://github.com/benletchford/systemless/commit/b703a888a0ad69599688a5e1e454cf1e43d0c607))
* fill kiosk margins around centered game surfaces ([#357](https://github.com/benletchford/systemless/issues/357)) ([17afc8d](https://github.com/benletchford/systemless/commit/17afc8def41b9bdd243e8f755b51c73106222d65))
* handle Control Strip Dispatch selectors safely ([4e45f22](https://github.com/benletchford/systemless/commit/4e45f22f7e385058d68422bd20028f769c4c7a9a))
* honor live control visibility when redrawing dialogs ([#386](https://github.com/benletchford/systemless/issues/386)) ([a2f949b](https://github.com/benletchford/systemless/commit/a2f949bfa0e157c7ca2d6daae92f19ece2400688))
* honor resolved color GrafPort pixel fields ([#356](https://github.com/benletchford/systemless/issues/356)) ([5ff3fe4](https://github.com/benletchford/systemless/commit/5ff3fe4a3d0a14b24cb25883a00a48cfa7ee1ac1))
* implement HighLevelHFSDispatch FSpOpenRF ([3d07591](https://github.com/benletchford/systemless/commit/3d075913af604dd83b11f2ac3ad6c4076ab1f631))
* install per-device gamma tables for indexed display output ([#367](https://github.com/benletchford/systemless/issues/367)) ([4ba0d2a](https://github.com/benletchford/systemless/commit/4ba0d2aa4374306b48cd11fe2ca12fce577d2647))
* keep FSpOpenRF lookups within the FSSpec ([#362](https://github.com/benletchford/systemless/issues/362)) ([890b85c](https://github.com/benletchford/systemless/commit/890b85cf6a2e2f08b0268f8122add987120213b9))
* keep WaitNextEvent asleep for eventless mouse movement ([#363](https://github.com/benletchford/systemless/issues/363)) ([da3db94](https://github.com/benletchford/systemless/commit/da3db94044e25c2f67dfbc078e567e12b0a06bcd))
* normalize the legacy magenta color-plane alias ([#355](https://github.com/benletchford/systemless/issues/355)) ([ef276d4](https://github.com/benletchford/systemless/commit/ef276d4e6f42438411bb9f0ca99538114d3f63b0))
* prefer Finder SIZE launch overrides ([#392](https://github.com/benletchford/systemless/issues/392)) ([2470f3e](https://github.com/benletchford/systemless/commit/2470f3ee7668ad60819da1970d6658a9618638fe))
* preserve legacy WDEF bounds for color windows ([#389](https://github.com/benletchford/systemless/issues/389)) ([b0c7b14](https://github.com/benletchford/systemless/commit/b0c7b14bfb209fe3890998654e0221545e0ab138))
* preserve logical palettes across quantized fades ([#353](https://github.com/benletchford/systemless/issues/353)) ([2d904a4](https://github.com/benletchford/systemless/commit/2d904a4080e424d0296b181df282c2e7cf9a73ee))
* preserve MacRoman HFS volume names ([#364](https://github.com/benletchford/systemless/issues/364)) ([7fd95cc](https://github.com/benletchford/systemless/commit/7fd95cc6ab80cb9391550b6b38f2c190dac2d4cf))
* preserve the physical palette after client-ID SetEntries fades ([#388](https://github.com/benletchford/systemless/issues/388)) ([2f0f448](https://github.com/benletchford/systemless/commit/2f0f448c525e8ae9d78d4576dac3c31d9b8f0511))
* render installed multicolor pixel patterns ([#366](https://github.com/benletchford/systemless/issues/366)) ([24c8afa](https://github.com/benletchford/systemless/commit/24c8afa737fb55b93222cc61ab365f45659954fe))
* resolve offscreen port colors against their pixmap table ([#390](https://github.com/benletchford/systemless/issues/390)) ([bd7baa4](https://github.com/benletchford/systemless/commit/bd7baa4dbeadea240a109409c3d87bd48f80f636))
* restore the native LoadSeg caller stack ([#359](https://github.com/benletchford/systemless/issues/359)) ([4530150](https://github.com/benletchford/systemless/commit/45301509568c1653431120cd67b85ec430647c09))
* retain ModalDialog click ownership through release ([#365](https://github.com/benletchford/systemless/issues/365)) ([e55b5dc](https://github.com/benletchford/systemless/commit/e55b5dcd44dffc430614ebf7d813d309e82a667b))
* return the application FCB refnum from HomeResFile ([73f09d4](https://github.com/benletchford/systemless/commit/73f09d4202f900d83a6952227dd05008056fa40a))
* stop APM type matching at the string terminator ([#361](https://github.com/benletchford/systemless/issues/361)) ([497637b](https://github.com/benletchford/systemless/commit/497637bf7fd4311033e1e00b383158bde3b1bd70))


### Performance Improvements

* fast-forward additional TickCount delay loops ([#351](https://github.com/benletchford/systemless/issues/351)) ([b4b7f8a](https://github.com/benletchford/systemless/commit/b4b7f8a142fd0b21d19a229aaf53ce0cd7f91656))
* fast-forward headless null-event cycles ([#352](https://github.com/benletchford/systemless/issues/352)) ([146d09c](https://github.com/benletchford/systemless/commit/146d09c1d80a97da91a6adde76395cfbbfd333f9))

## [0.11.5](https://github.com/benletchford/systemless/compare/v0.11.4...v0.11.5) (2026-08-04)


### Performance Improvements

* **macos:** cache native application identity by path ([3274502](https://github.com/benletchford/systemless/commit/327450269aee81bfb25226b1e035b27b0de3f2f5))
* **quickdraw:** bulk-decode guest color tables ([7201fd0](https://github.com/benletchford/systemless/commit/7201fd0e54804a55010727f09303591d1d577965))

## [0.11.4](https://github.com/benletchford/systemless/compare/v0.11.3...v0.11.4) (2026-08-03)


### Bug Fixes

* restore resource allocation for classic games ([#300](https://github.com/benletchford/systemless/issues/300)) ([ba6c642](https://github.com/benletchford/systemless/commit/ba6c64253184df0cb87d14116e39b8ab59ad780f))

## [0.11.3](https://github.com/benletchford/systemless/compare/v0.11.2...v0.11.3) (2026-08-03)


### Bug Fixes

* preserve resident resources and stable trap startup state ([0a0595f](https://github.com/benletchford/systemless/commit/0a0595fc866509658c1135004e3de224cd7f3aea))
* support Spectre Supreme startup and menu input ([e2844d8](https://github.com/benletchford/systemless/commit/e2844d8acc896d8e8c9fe95bdf128d590d0f339c))
* support Warcraft II palette and icon probes ([d1060f8](https://github.com/benletchford/systemless/commit/d1060f8b6dc49b1db951d3006ea99b35846b9c53))

## [0.11.2](https://github.com/benletchford/systemless/compare/v0.11.1...v0.11.2) (2026-08-03)


### Bug Fixes

* clear stale resource errors after GetPicture lookups ([0c2758e](https://github.com/benletchford/systemless/commit/0c2758e17e8caeb54763f262ea6f8f5718557f69))
* restore promoted window after dialog disposal ([475bcb2](https://github.com/benletchford/systemless/commit/475bcb2448b3ac1b2a4513988a4f91165038cd6a))
* restore resource and Edition Manager startup state ([5b25c29](https://github.com/benletchford/systemless/commit/5b25c29f10953cc642d75ea9e4ca73db941c09fc))


### Performance Improvements

* buffer common 8-bit CopyMask rows ([66a05ff](https://github.com/benletchford/systemless/commit/66a05ffb91d170e003912d78f87905f4542cb8f7))

## [0.11.1](https://github.com/benletchford/systemless/compare/v0.11.0...v0.11.1) (2026-07-31)


### Bug Fixes

* update explicit animated palette entries ([#263](https://github.com/benletchford/systemless/issues/263)) ([9635b25](https://github.com/benletchford/systemless/commit/9635b254b92c3cb7f7712a2d9b654bb9d9c018ab))

## [0.11.0](https://github.com/benletchford/systemless/compare/v0.10.3...v0.11.0) (2026-07-31)


### Features

* use guest application icons on macOS ([5deef33](https://github.com/benletchford/systemless/commit/5deef33fbf462db5151fc8ceca70a23da30007ee))


### Bug Fixes

* preserve classic application presentation and timing ([530ff09](https://github.com/benletchford/systemless/commit/530ff0942fca3a37ac392a202daa72c1855ee740))
* scale classic application icons appropriately on macOS ([6eb8597](https://github.com/benletchford/systemless/commit/6eb8597df92c973e6b3a3202d428ef6964e04675))

## [0.10.3](https://github.com/benletchford/systemless/compare/v0.10.2...v0.10.3) (2026-07-31)


### Bug Fixes

* upgrade m68k to 0.3.2 and correct Rustdocs ([43ddd9c](https://github.com/benletchford/systemless/commit/43ddd9c271704151733624d8f85ee7fab7a3ff03))

## [0.10.2](https://github.com/benletchford/systemless/compare/v0.10.1...v0.10.2) (2026-07-31)


### Bug Fixes

* upgrade m68k to 0.3.1 with native JIT ([67f1ad2](https://github.com/benletchford/systemless/commit/67f1ad2a9e0685fc9967fa366ee35519f3b81ed3))

## [0.10.1](https://github.com/benletchford/systemless/compare/v0.10.0...v0.10.1) (2026-07-31)


### Bug Fixes

* upgrade m68k to 0.3.0 ([f91f7a1](https://github.com/benletchford/systemless/commit/f91f7a1672b14dad3d93cd533f15e0d2aa7e03d6))

## [0.10.0](https://github.com/benletchford/systemless/compare/v0.9.18...v0.10.0) (2026-07-31)


### Features

* execute application control definition procedures ([9639669](https://github.com/benletchford/systemless/commit/963966934f1c1513cb61ccf2cd81d32ddee40694))

## [0.9.18](https://github.com/benletchford/systemless/compare/v0.9.17...v0.9.18) (2026-07-31)


### Bug Fixes

* recalculate window occlusion after resizing ([919c0b2](https://github.com/benletchford/systemless/commit/919c0b268a05717c399e7b51f6c92e836d48edf2))

## [0.9.17](https://github.com/benletchford/systemless/compare/v0.9.16...v0.9.17) (2026-07-31)


### Bug Fixes

* expose offscreen pixel pointers while locked ([c309d76](https://github.com/benletchford/systemless/commit/c309d76e405c690ede416b9a35969dd50e4d911d))

## [0.9.16](https://github.com/benletchford/systemless/compare/v0.9.15...v0.9.16) (2026-07-31)


### Bug Fixes

* preserve reserved indices for animated palette entries ([#242](https://github.com/benletchford/systemless/issues/242)) ([3538c92](https://github.com/benletchford/systemless/commit/3538c9226d4268afe36b72f114c28b4307c90ed3))

## [0.9.15](https://github.com/benletchford/systemless/compare/v0.9.14...v0.9.15) (2026-07-31)


### Bug Fixes

* replay recorded QuickDraw pictures with their requested palettes ([6dabd84](https://github.com/benletchford/systemless/commit/6dabd84289f9a20f63f31bee10e258bc2eba0112))

## [0.9.14](https://github.com/benletchford/systemless/compare/v0.9.13...v0.9.14) (2026-07-30)


### Bug Fixes

* ignore stale ModalDialog filter results before callbacks ([c8846b9](https://github.com/benletchford/systemless/commit/c8846b9badb6e17b0058a9673b15eb01f27b21e0))
* shadow witnessed boot ROM bytes ([d73a15f](https://github.com/benletchford/systemless/commit/d73a15fab08de68093a9544b8d21bbb75e8d34f2))


### Performance Improvements

* update retained dialog snapshots by damaged region ([22bfe1c](https://github.com/benletchford/systemless/commit/22bfe1cfc919de569f3cdcc29ec885acdca659a4))

## [0.9.13](https://github.com/benletchford/systemless/compare/v0.9.12...v0.9.13) (2026-07-30)


### Bug Fixes

* use ROM inverse tables for 4-bit RGB shapes ([d701e95](https://github.com/benletchford/systemless/commit/d701e95b9630d51bc7441077e0a29d54c88991e5))

## [0.9.12](https://github.com/benletchford/systemless/compare/v0.9.11...v0.9.12) (2026-07-30)


### Bug Fixes

* match ROM 4-bit to 8-bit color mapping ([21bdeff](https://github.com/benletchford/systemless/commit/21bdeffb3b349f2a1a17cb5ef91bc6181ce08e1b))

## [0.9.11](https://github.com/benletchford/systemless/compare/v0.9.10...v0.9.11) (2026-07-30)


### Bug Fixes

* match ROM color mapping for standard 4-bit GWorlds ([aaaf689](https://github.com/benletchford/systemless/commit/aaaf6891241f69fe7cbf9b968e320cf7845fb211))

## [0.9.10](https://github.com/benletchford/systemless/compare/v0.9.9...v0.9.10) (2026-07-30)


### Bug Fixes

* honor packed indexed CopyBits transfer modes ([#220](https://github.com/benletchford/systemless/issues/220)) ([63e441c](https://github.com/benletchford/systemless/commit/63e441ce6ea6338e09d48152acf5762aac400f9e))

## [0.9.9](https://github.com/benletchford/systemless/compare/v0.9.8...v0.9.9) (2026-07-30)


### Bug Fixes

* initialize packed indexed color tables by depth ([#217](https://github.com/benletchford/systemless/issues/217)) ([7eca3e8](https://github.com/benletchford/systemless/commit/7eca3e89df695a8539951c6873659346df187176))

## [0.9.8](https://github.com/benletchford/systemless/compare/v0.9.7...v0.9.8) (2026-07-30)


### Bug Fixes

* correct FixRound Pascal stack discipline ([#214](https://github.com/benletchford/systemless/issues/214)) ([2f66eed](https://github.com/benletchford/systemless/commit/2f66eed0ff6d185964618d6fb1dd6cfa9dcf048d))

## [0.9.7](https://github.com/benletchford/systemless/compare/v0.9.6...v0.9.7) (2026-07-30)


### Bug Fixes

* preserve Finder metadata in web packages ([#211](https://github.com/benletchford/systemless/issues/211)) ([b52cb58](https://github.com/benletchford/systemless/commit/b52cb5867fda53da3c2078c820ba185fbd2af664))

## [0.9.6](https://github.com/benletchford/systemless/compare/v0.9.5...v0.9.6) (2026-07-30)


### Bug Fixes

* translate indexed CopyBits across differing depths ([#208](https://github.com/benletchford/systemless/issues/208)) ([c53b5d4](https://github.com/benletchford/systemless/commit/c53b5d40696a7ebaabd31afc046938a9adc8a55e))

## [0.9.5](https://github.com/benletchford/systemless/compare/v0.9.4...v0.9.5) (2026-07-30)


### Bug Fixes

* gate launch Apple events on the SIZE capability ([#203](https://github.com/benletchford/systemless/issues/203)) ([f975d51](https://github.com/benletchford/systemless/commit/f975d51b762636ff49e4e729cf682109b326a81f))
* install canonical indexed palettes when changing depth ([#205](https://github.com/benletchford/systemless/issues/205)) ([129fd4d](https://github.com/benletchford/systemless/commit/129fd4d7ad3d5534a181f025df3ef8fc655533e6))
* render Toolbox chrome through packed indexed framebuffers ([#206](https://github.com/benletchford/systemless/issues/206)) ([7ba1b86](https://github.com/benletchford/systemless/commit/7ba1b863b7d790718590de06be53e2e6c30d1303))
* resolve bitmap strikes through FOND associations ([#200](https://github.com/benletchford/systemless/issues/200)) ([8cde076](https://github.com/benletchford/systemless/commit/8cde0768739c970ba645b5e6f55ffd0f3afa0707))
* return canonical byte Booleans from dialog event traps ([#204](https://github.com/benletchford/systemless/issues/204)) ([bf9e9f2](https://github.com/benletchford/systemless/commit/bf9e9f212566156a7a88aa66d9eff76a10147889))

## [0.9.4](https://github.com/benletchford/systemless/compare/v0.9.3...v0.9.4) (2026-07-30)


### Bug Fixes

* preserve explicit palette indices during indexed blits ([#194](https://github.com/benletchford/systemless/issues/194)) ([4a2429d](https://github.com/benletchford/systemless/commit/4a2429dac44a9bfad81edbbbe2b47e2c5e88c048))

## [0.9.3](https://github.com/benletchford/systemless/compare/v0.9.2...v0.9.3) (2026-07-30)


### Bug Fixes

* create and resolve minimal full-path aliases ([#191](https://github.com/benletchford/systemless/issues/191)) ([1c0f4c2](https://github.com/benletchford/systemless/commit/1c0f4c2f14f9b5c24edf5a9df541968f82802a9f))

## [0.9.2](https://github.com/benletchford/systemless/compare/v0.9.1...v0.9.2) (2026-07-30)


### Bug Fixes

* record 8-bit ClosePicture snapshots without overflow ([1fa7a56](https://github.com/benletchford/systemless/commit/1fa7a56e3177324ff93e7d20d0590dbe430cef06))
* restore offscreen PixMap base handles ([59e772f](https://github.com/benletchford/systemless/commit/59e772fe4beda4d2d50a5f29aaab565f22cade98))

## [0.9.1](https://github.com/benletchford/systemless/compare/v0.9.0...v0.9.1) (2026-07-30)


### Bug Fixes

* strip synthetic Unix volumes from HFS paths ([aaad830](https://github.com/benletchford/systemless/commit/aaad83056b225fcb6c73dd67581fcd73d262e573))

## [0.9.0](https://github.com/benletchford/systemless/compare/v0.8.8...v0.9.0) (2026-07-30)


### Features

* pack multi-source HFS game releases for web ([#171](https://github.com/benletchford/systemless/issues/171)) ([db4281e](https://github.com/benletchford/systemless/commit/db4281e553083bb5086db7376ac4c47e17867a1d))


### Bug Fixes

* deliver mouse input to custom ADB service routines ([#149](https://github.com/benletchford/systemless/issues/149)) ([956efd2](https://github.com/benletchford/systemless/commit/956efd229bb5295091874b62a1de8722ad01b029))
* **dialog:** retain packed indexed snapshots ([#173](https://github.com/benletchford/systemless/issues/173)) ([03dd973](https://github.com/benletchford/systemless/commit/03dd973fca8e3510c7d14c6d72fbae693a1ad0e9))
* **files:** preserve relative pathname components ([#159](https://github.com/benletchford/systemless/issues/159)) ([70e5a21](https://github.com/benletchford/systemless/commit/70e5a21e0f66d702d2db8ccd1cf1812d9ef50c37))
* **files:** preserve slashes in HFS filenames ([#155](https://github.com/benletchford/systemless/issues/155)) ([4fa5eb2](https://github.com/benletchford/systemless/commit/4fa5eb294deab41682e7d4870aaed74f76add70e))
* **files:** resolve the volume root through its parent ID ([#153](https://github.com/benletchford/systemless/issues/153)) ([36557bc](https://github.com/benletchford/systemless/commit/36557bcd9031f5184434101781c1a5cfef363c0d))
* **loader:** mount MacBinary application forks ([#151](https://github.com/benletchford/systemless/issues/151)) ([eb0c1a8](https://github.com/benletchford/systemless/commit/eb0c1a845025237101a02ef2d6660e1410c8a446))
* **quickdraw:** encode indexed ClosePicture snapshots ([#175](https://github.com/benletchford/systemless/issues/175)) ([eab646a](https://github.com/benletchford/systemless/commit/eab646a34c36dabac6678ab681a38abcf6a69f58))
* **quickdraw:** translate screen CopyBits rectangles ([#162](https://github.com/benletchford/systemless/issues/162)) ([f34d52d](https://github.com/benletchford/systemless/commit/f34d52d38fa54a84d7489c4632dffdd000482f2d))
* render 4-bit indexed display modes ([#170](https://github.com/benletchford/systemless/issues/170)) ([9267b22](https://github.com/benletchford/systemless/commit/9267b22761cc87097093f82e81f19227adbeaba9))
* translate indexed colors in CopyMask ([#167](https://github.com/benletchford/systemless/issues/167)) ([22fc694](https://github.com/benletchford/systemless/commit/22fc6941a4a5ba5087e77d5ccfdfb3ded19da679))
* use canonical Pascal booleans in Standard File replies ([#157](https://github.com/benletchford/systemless/issues/157)) ([d5e8ae0](https://github.com/benletchford/systemless/commit/d5e8ae03e95c96ca9d2732d15ac8792e351a1b8d))
* **window:** erase newly exposed window content ([#158](https://github.com/benletchford/systemless/issues/158)) ([e4759cd](https://github.com/benletchford/systemless/commit/e4759cda5b5a3f9a1a7904d846975ae03f7e41ba))
* **window:** redraw window pictures during update scans ([#164](https://github.com/benletchford/systemless/issues/164)) ([41a8d3f](https://github.com/benletchford/systemless/commit/41a8d3fe551db859ee76f8388561ce1d5a4e53b0))
* **window:** refresh saved-under desktop pixels ([#166](https://github.com/benletchford/systemless/issues/166)) ([3e57ba6](https://github.com/benletchford/systemless/commit/3e57ba6b18c90e930702f8a8c543f0ace91ea811))

## [0.8.8](https://github.com/benletchford/systemless/compare/v0.8.7...v0.8.8) (2026-07-29)


### Bug Fixes

* restore retained modal backgrounds using dialog bounds ([95ff71c](https://github.com/benletchford/systemless/commit/95ff71c31e9905197a8414d4f1e6535e8b903b7b))

## [0.8.7](https://github.com/benletchford/systemless/compare/v0.8.6...v0.8.7) (2026-07-29)


### Bug Fixes

* **loader:** prefer user applications over System Folder utilities ([722f76e](https://github.com/benletchford/systemless/commit/722f76ead301cf4f1feeb6d789271756ebbb69df))

## [0.8.6](https://github.com/benletchford/systemless/compare/v0.8.5...v0.8.6) (2026-07-29)


### Performance Improvements

* **runner:** prove and park exact idle cycles ([8cd95d7](https://github.com/benletchford/systemless/commit/8cd95d73a733a9fe21d546b3cba6d5b9df3372c5))
* **runner:** support signed BLT and computed-deadline TickCount waits ([bfe2ded](https://github.com/benletchford/systemless/commit/bfe2ded6a5f691e4b82b1fad17fc239f08982398))

## [0.8.5](https://github.com/benletchford/systemless/compare/v0.8.4...v0.8.5) (2026-07-29)


### Bug Fixes

* honor guest-updated mouse coordinates in GetMouse ([#139](https://github.com/benletchford/systemless/issues/139)) ([d47e2e9](https://github.com/benletchford/systemless/commit/d47e2e9ba454aee94f286ca6abefb967a66e7822))

## [0.8.4](https://github.com/benletchford/systemless/compare/v0.8.3...v0.8.4) (2026-07-29)


### Bug Fixes

* preserve the current GrafPort when creating dialogs ([6fa282f](https://github.com/benletchford/systemless/commit/6fa282f66e3a3cd8a2759dab71dd58700720d7ac))

## [0.8.3](https://github.com/benletchford/systemless/compare/v0.8.2...v0.8.3) (2026-07-29)


### Bug Fixes

* render generated RGB pixel patterns in color ports ([5afe04d](https://github.com/benletchford/systemless/commit/5afe04d8c916f2c775d7655054a188936c1fbd44))

## [0.8.2](https://github.com/benletchford/systemless/compare/v0.8.1...v0.8.2) (2026-07-28)


### Bug Fixes

* honor MakeRGBPat colors in FillCRect ([#128](https://github.com/benletchford/systemless/issues/128)) ([7f50f52](https://github.com/benletchford/systemless/commit/7f50f527dd619eddd6ebe307919aca5f321a9ed9))
* implement Pack4 binary-to-decimal conversion ([#126](https://github.com/benletchford/systemless/issues/126)) ([3623732](https://github.com/benletchford/systemless/commit/3623732d066b75c786c204ed1654db2c6bb9c023))
* resolve legacy PBCreate through working directory references ([#124](https://github.com/benletchford/systemless/issues/124)) ([185b8c5](https://github.com/benletchford/systemless/commit/185b8c583287a2588b094e4e31f991f78f59b95e))
* treat nonzero control visibility values as visible ([#122](https://github.com/benletchford/systemless/issues/122)) ([133c00a](https://github.com/benletchford/systemless/commit/133c00ac4b7b7248575a7b24248f6a0b0e2f58c7))

## [0.8.1](https://github.com/benletchford/systemless/compare/v0.8.0...v0.8.1) (2026-07-28)


### Bug Fixes

* accept a Pascal BOOLEAN parameter in either byte of its stack slot ([#114](https://github.com/benletchford/systemless/issues/114)) ([d4ea0f5](https://github.com/benletchford/systemless/commit/d4ea0f5f9f3e8437d3346f57ba1138a6490d8b25))
* preserve Sound Manager Pascal callback and dispatch frames ([#120](https://github.com/benletchford/systemless/issues/120)) ([a89d233](https://github.com/benletchford/systemless/commit/a89d233a75b2cde2c964f7be61b76b5583bef70a))
* resolve control title ink against the live device colour table ([#119](https://github.com/benletchford/systemless/issues/119)) ([c804e91](https://github.com/benletchford/systemless/commit/c804e91b5524a183dad2ce89fa6afdb194a09218))

## [0.8.0](https://github.com/benletchford/systemless/compare/v0.7.1...v0.8.0) (2026-07-27)


### Features

* implement the full Thread Manager selector table for cooperative threads ([0545f62](https://github.com/benletchford/systemless/commit/0545f62de193dbb2d4c1cf8279083243e1246cc3))

## [0.7.1](https://github.com/benletchford/systemless/compare/v0.7.0...v0.7.1) (2026-07-27)


### Bug Fixes

* activate queued launches when caller exits ([0b8c40a](https://github.com/benletchford/systemless/commit/0b8c40a1890714df2e05d53b9fed44d48eb7f2c6))
* correct sound doubleback argument order ([f390999](https://github.com/benletchford/systemless/commit/f39099990f19c64e90c297c1d584a900e0d0ef64))
* deliver asynchronous file read completions ([49759a4](https://github.com/benletchford/systemless/commit/49759a4ffb1e2eeea5983f1de3a56a8942ab4e67))
* honor ShieldCursor intersection visibility ([b27a3f3](https://github.com/benletchford/systemless/commit/b27a3f386ab7c9c214c343a0574631d531bb5b11))
* initialize QuickDraw cursor vectors ([5117ad4](https://github.com/benletchford/systemless/commit/5117ad435dedb15959dd87dc3e9e30518c77fc19))
* initialize shield cursor vector ([66faeef](https://github.com/benletchford/systemless/commit/66faeef1fe7af40f6254fa8b552d172c00e2fde6))
* initialize show cursor vector ([0670e85](https://github.com/benletchford/systemless/commit/0670e85fa26924cbd822bde50f4a433e06491829))
* initialize swap mmu trap vector ([b3395ae](https://github.com/benletchford/systemless/commit/b3395ae23e5e125000268fd5ec9d3cfb04214587))
* interleave sub-vbl timer callbacks ([0d3a3fe](https://github.com/benletchford/systemless/commit/0d3a3fe2e8e87c4051f71a066f85967ab5c94b25))
* isolate sound callback trampolines ([3c08c52](https://github.com/benletchford/systemless/commit/3c08c523978e2cb92c0e4e2378750d8976d02e22))
* **memory:** keep application allocations inside the zone boundary ([76dbf1e](https://github.com/benletchford/systemless/commit/76dbf1e0f5f8c29effd33b8f695b78a2317780c1))
* pace self-reprimed timer tasks ([48494fb](https://github.com/benletchford/systemless/commit/48494fb274f846e23e2459a69b245c881bb6773a))
* preserve concurrent timer callbacks ([69e6402](https://github.com/benletchford/systemless/commit/69e64027a54bbef4808c6791023d869ff082ec55))
* preserve sub-vbl timer deadlines ([284495f](https://github.com/benletchford/systemless/commit/284495f9361699a4f3a8cb26d5cd263143d7ed4c))
* prioritize overdue timer callbacks ([95fad8f](https://github.com/benletchford/systemless/commit/95fad8fed3ef017e74469e7adc9b0ddbce6d7a28))
* **quickdraw:** support packed indexed CopyBits sources ([d211fa4](https://github.com/benletchford/systemless/commit/d211fa4202d73d883c1ab991bb16a38549d76ecf))
* resolve menu definition procedure handles ([25f7829](https://github.com/benletchford/systemless/commit/25f7829412190352ff4dbac737b399fd0b145d6e))
* stabilize centered fullscreen margins ([f5fffd0](https://github.com/benletchford/systemless/commit/f5fffd0696ae6262485553a3ca9ae23bf9c7293e))
* **window:** expose the legacy window manager port layout ([dcac552](https://github.com/benletchford/systemless/commit/dcac552bc0d1956c893c5b8e330463d2b4bd731e))

## [0.7.0](https://github.com/benletchford/systemless/compare/v0.6.0...v0.7.0) (2026-07-26)


### Features

* add cooperative Thread Manager support ([04348cd](https://github.com/benletchford/systemless/commit/04348cde025509160b5becbec81061b7dff3262f))


### Bug Fixes

* terminate indexed volume enumeration ([40bd9a3](https://github.com/benletchford/systemless/commit/40bd9a3124a4dc297b039a0fba8c67dac84c27d8))

## [0.6.0](https://github.com/benletchford/systemless/compare/v0.5.0...v0.6.0) (2026-07-26)


### Features

* **macos:** mirror guest menus in the menu bar ([c27d671](https://github.com/benletchford/systemless/commit/c27d6716afb4d6981629d980620bcab38b62f918))


### Bug Fixes

* **gui:** derive viewports from guest-drawn frames ([9ab5281](https://github.com/benletchford/systemless/commit/9ab528128b5d5fb91f7e7522aa34bf54cff19694))
* **quickdraw:** frame complex regions as outlines ([67d69dd](https://github.com/benletchford/systemless/commit/67d69dd8725424767948d3a370b6f985687bdee5))
* **quickdraw:** honor application bitmap fonts and face widths ([cbd7f67](https://github.com/benletchford/systemless/commit/cbd7f67f1e443f18361e70ef6672830218d2606a))
* **quickdraw:** implement SetPortPix for color ports ([d704cc5](https://github.com/benletchford/systemless/commit/d704cc532b526b0b59ceeee44191decd89337b78))
* **quickdraw:** preserve destinations in colored OR modes ([7b0dde1](https://github.com/benletchford/systemless/commit/7b0dde1b8fdfa2aaa63bf0d9365890449019e048))
* **quickdraw:** preserve polygon boundaries in regions ([00026af](https://github.com/benletchford/systemless/commit/00026afbe257d78ea1aa6793fe628dbafeafe49d))


### Performance Improvements

* **macos:** stage only visible guest framebuffer rows ([ce355f9](https://github.com/benletchford/systemless/commit/ce355f9f69045444c5a0edd494aad55033c832fe))
* **runner:** fast-forward signed TickCount waits ([c42dbb3](https://github.com/benletchford/systemless/commit/c42dbb368787ecbab1a5674b3fa6701509b7f3f9))

## [0.5.0](https://github.com/benletchford/systemless/compare/v0.4.2...v0.5.0) (2026-07-26)


### Features

* document Homebrew installation ([be7742f](https://github.com/benletchford/systemless/commit/be7742f030dc63289b84141c328fef3d8f1aacdc))


### Bug Fixes

* decode menu titles and item text as Mac Roman instead of UTF-8 ([b719dc4](https://github.com/benletchford/systemless/commit/b719dc4112cdae50475b91c353e6095f269653f4))
* draw dimmed menu items, dividers and title metrics the way System 7.5.3 does ([0230216](https://github.com/benletchford/systemless/commit/0230216bfe16c80760b56a658a90131722285a55))
* preserve direct framebuffer output after pointer resize failures ([4ce0903](https://github.com/benletchford/systemless/commit/4ce090384ac148cc00dcf8a48be0338b62719a8d))
* preserve VBL queues and Color QuickDraw indices for direct-drawing games ([be22899](https://github.com/benletchford/systemless/commit/be228991c3064444ccc686b849ce7ef093223dea))

## [0.4.2](https://github.com/benletchford/systemless/compare/v0.4.1...v0.4.2) (2026-07-26)


### Bug Fixes

* preserve application-painted dialog pixels during framebuffer redraws ([954de06](https://github.com/benletchford/systemless/commit/954de06e6ccc76d57dc28e1b5740cdf3152b5f56))
* stop ModalDialog repainting a dialog the application already drew itself ([a56286d](https://github.com/benletchford/systemless/commit/a56286d908d1ed5850d857a571c09ca2056bb514))

## [0.4.1](https://github.com/benletchford/systemless/compare/v0.4.0...v0.4.1) (2026-07-25)


### Bug Fixes

* dim a disabled menu title with the gray pattern instead of hiding it ([92d9246](https://github.com/benletchford/systemless/commit/92d92466037b45d703e6045ad09dad25e03909d4))
* keep the menu-bar exclusion out of a window's content region so moved windows do not lose their top rows ([3ba5c08](https://github.com/benletchford/systemless/commit/3ba5c083b51c5e72e0cc7ea34880c32e10a241e5))
* populate the 68k exception vector table so a stray deref of address 0 lands in ROM space ([b764613](https://github.com/benletchford/systemless/commit/b7646136281e71bc02879327047a9ce70fbef60f))
* report a loaded resource handle's exact byte count from GetHandleSize ([e85894e](https://github.com/benletchford/systemless/commit/e85894e88544f84fca84d50c417aec361af3cab0))
* subtract front windows from visRgn so a background repaint cannot erase a modal dialog ([2420fb0](https://github.com/benletchford/systemless/commit/2420fb06c5a1b5b64f2872096afba568c99635e7))

## [0.4.0](https://github.com/benletchford/systemless/compare/v0.3.0...v0.4.0) (2026-07-25)


### Features

* play QuickTime movies on a timeline with cvid, rle and smc image decoders ([37b62a7](https://github.com/benletchford/systemless/commit/37b62a76b7ebaede548b281ecc83ee1a027e6d7b))

## [0.3.0](https://github.com/benletchford/systemless/compare/v0.2.5...v0.3.0) (2026-07-25)


### Features

* show the retro computer menu mark in 68k games ([e96d4a5](https://github.com/benletchford/systemless/commit/e96d4a5691324cf7d1a62d4a0433b554f2ca5677))


### Bug Fixes

* .release-please-manifest.json ([4a7fe45](https://github.com/benletchford/systemless/commit/4a7fe45a08df5799a79808af2e0f8a8cc72d3096))
* **audio:** recover buffered lead after frontend stalls ([3cf0a5f](https://github.com/benletchford/systemless/commit/3cf0a5fb54e247801fd860d3840aff15b95ba1dd))
* **build:** keep the Metal presenter out of automatic binaries ([ad9a089](https://github.com/benletchford/systemless/commit/ad9a089eeb47b9cfd6c3943660a40f5d275dffa7))
* **deps:** update m68k to 0.2.4 ([a7882de](https://github.com/benletchford/systemless/commit/a7882de4d7eecd0e4cf3bd86c31654de4dc93933))
* **event:** initialize classic double-click interval ([a95e9d3](https://github.com/benletchford/systemless/commit/a95e9d3de6260c1a0f0504cde7d0ab9f234502ce))
* **event:** initialize classic double-click interval ([d42f3d1](https://github.com/benletchford/systemless/commit/d42f3d177c15108b29181724d708c14a0aa0295a))
* **gui:** detect and cache centered game viewports ([74a3391](https://github.com/benletchford/systemless/commit/74a33911dc1200da87578d0eff1e0f2db6cbd88e))
* **gui:** preserve cached crop during startup detection ([3ab59ab](https://github.com/benletchford/systemless/commit/3ab59ab76c292a06803c99ec2b9c8f0e93c01c8f))
* **gui:** preserve transactional resizing with async presentation ([891562c](https://github.com/benletchford/systemless/commit/891562cf0b216d00d5716437cb0251b3645aea24))
* **gui:** reveal transient dialogs without resize bounce ([b3d4164](https://github.com/benletchford/systemless/commit/b3d4164f127b685e4d2169aa9d6432d54e0687a5))
* honor the system event mask when posting key-up events ([e81e949](https://github.com/benletchford/systemless/commit/e81e949e9bab24b8a806b59992891cf664cf19e7))
* **macos:** drain Metal worker autoreleases ([e98664d](https://github.com/benletchford/systemless/commit/e98664d97390136bc5e79d5c16d8d3086a1681de))
* **macos:** drain Metal worker autoreleases ([a8e0f1b](https://github.com/benletchford/systemless/commit/a8e0f1b459e5732be0632dbe68e1658a97ac892f))
* **memory:** validate RecoverHandle master-pointer slots ([b72ea7d](https://github.com/benletchford/systemless/commit/b72ea7ddb50f784fdf49d67f4322b09bd4805f0d))
* **memory:** validate RecoverHandle master-pointer slots ([53c1fa7](https://github.com/benletchford/systemless/commit/53c1fa7afdfa6a018023a017739ad36cc5e1693d))
* preserve guest menu enable flags in MORE ([04370de](https://github.com/benletchford/systemless/commit/04370de9f5575c043d21303ae5b73e0c3d81dbcc))
* select menu item at mouse release ([c2177b1](https://github.com/benletchford/systemless/commit/c2177b12f12a5c18681328bb1b9a0b48e4f1cc19))
* suppress duplicate host keydown events ([38155a2](https://github.com/benletchford/systemless/commit/38155a2b6d3a015ec828588300eb7b58ac14980b))
* suppress standalone modifier key events ([f286228](https://github.com/benletchford/systemless/commit/f2862283ad72c1e654d4cf3a08364d95f4a07b65))
* terminate exhausted Marathon terminal style insertions ([30f49a4](https://github.com/benletchford/systemless/commit/30f49a4d929c37ecd9e5c4de14b2ead5af420d89))


### Performance Improvements

* **gui:** decode guest framebuffers in Metal ([fca3ecc](https://github.com/benletchford/systemless/commit/fca3ecccf0a88570c5542e4d9eb8faf290f810d1))
* **gui:** decode guest framebuffers in Metal ([76c3925](https://github.com/benletchford/systemless/commit/76c3925496d43097b2cc016b9f2208c3782d14f2))
* **gui:** decouple Metal presentation from emulation ([b165448](https://github.com/benletchford/systemless/commit/b165448b317acae1d3bf51ce94174265dd166d79))
* **gui:** decouple Metal presentation from emulation ([e03f4b0](https://github.com/benletchford/systemless/commit/e03f4b02440b20703644ea86d804cd42dcd948dc)), closes [#23](https://github.com/benletchford/systemless/issues/23)
* **gui:** present macOS frames with Metal ([ec589a8](https://github.com/benletchford/systemless/commit/ec589a825fa6cc7dd0015558317dc70a4af7f9f4))
* **gui:** present macOS frames with Metal ([35cef58](https://github.com/benletchford/systemless/commit/35cef58733c62aa77211188533e2d212129efae5))
* **gui:** skip unchanged Metal guest frames ([812e1a7](https://github.com/benletchford/systemless/commit/812e1a749009da7824d91b65b2dbfb0832e304ec))
* **gui:** skip unchanged Metal guest frames ([d2ab17c](https://github.com/benletchford/systemless/commit/d2ab17c062f7a51859c106f94caddd6a07eabcd0))
* **runner:** fast-forward capped GUI TickCount waits ([7b9b940](https://github.com/benletchford/systemless/commit/7b9b940424f89b8ad70ff9d270fcf082ee642147))
* **runner:** fast-forward capped GUI TickCount waits ([721ab96](https://github.com/benletchford/systemless/commit/721ab96cccf836afa156b787e6d183ef28ac7ea3))
* **runner:** fast-forward TickCount BEQ waits ([6dc03b4](https://github.com/benletchford/systemless/commit/6dc03b430dc11ffd13c19c18ef583767c3abe6e6))
* **runner:** fast-forward TickCount BEQ waits ([22231e0](https://github.com/benletchford/systemless/commit/22231e0803c1b67f7fddca559f514a9cd359995c))

## [0.2.2](https://github.com/benletchford/systemless/compare/v0.2.1...v0.2.2) (2026-07-24)


### Bug Fixes

* **deps:** update m68k to 0.2.4 ([a7882de](https://github.com/benletchford/systemless/commit/a7882de4d7eecd0e4cf3bd86c31654de4dc93933))

## [0.2.1](https://github.com/benletchford/systemless/compare/v0.2.0...v0.2.1) (2026-07-24)


### Bug Fixes

* **macos:** drain Metal worker autoreleases ([e98664d](https://github.com/benletchford/systemless/commit/e98664d97390136bc5e79d5c16d8d3086a1681de))
* **macos:** drain Metal worker autoreleases ([a8e0f1b](https://github.com/benletchford/systemless/commit/a8e0f1b459e5732be0632dbe68e1658a97ac892f))


### Performance Improvements

* **runner:** fast-forward TickCount BEQ waits ([6dc03b4](https://github.com/benletchford/systemless/commit/6dc03b430dc11ffd13c19c18ef583767c3abe6e6))
* **runner:** fast-forward TickCount BEQ waits ([22231e0](https://github.com/benletchford/systemless/commit/22231e0803c1b67f7fddca559f514a9cd359995c))

## [0.2.0](https://github.com/benletchford/systemless/compare/v0.1.133...v0.2.0) (2026-07-23)


### Features

* show the retro computer menu mark in 68k games ([e96d4a5](https://github.com/benletchford/systemless/commit/e96d4a5691324cf7d1a62d4a0433b554f2ca5677))


### Bug Fixes

* preserve guest menu enable flags in MORE ([04370de](https://github.com/benletchford/systemless/commit/04370de9f5575c043d21303ae5b73e0c3d81dbcc))
* select menu item at mouse release ([c2177b1](https://github.com/benletchford/systemless/commit/c2177b12f12a5c18681328bb1b9a0b48e4f1cc19))
* suppress standalone modifier key events ([f286228](https://github.com/benletchford/systemless/commit/f2862283ad72c1e654d4cf3a08364d95f4a07b65))

## [0.1.133](https://github.com/benletchford/systemless/compare/v0.1.132...v0.1.133) (2026-07-23)


### Bug Fixes

* **audio:** recover buffered lead after frontend stalls ([3cf0a5f](https://github.com/benletchford/systemless/commit/3cf0a5fb54e247801fd860d3840aff15b95ba1dd))
* **build:** keep the Metal presenter out of automatic binaries ([ad9a089](https://github.com/benletchford/systemless/commit/ad9a089eeb47b9cfd6c3943660a40f5d275dffa7))
* **event:** initialize classic double-click interval ([a95e9d3](https://github.com/benletchford/systemless/commit/a95e9d3de6260c1a0f0504cde7d0ab9f234502ce))
* **event:** initialize classic double-click interval ([d42f3d1](https://github.com/benletchford/systemless/commit/d42f3d177c15108b29181724d708c14a0aa0295a))
* **gui:** detect and cache centered game viewports ([74a3391](https://github.com/benletchford/systemless/commit/74a33911dc1200da87578d0eff1e0f2db6cbd88e))
* **gui:** preserve cached crop during startup detection ([3ab59ab](https://github.com/benletchford/systemless/commit/3ab59ab76c292a06803c99ec2b9c8f0e93c01c8f))
* **gui:** preserve transactional resizing with async presentation ([891562c](https://github.com/benletchford/systemless/commit/891562cf0b216d00d5716437cb0251b3645aea24))
* **gui:** reveal transient dialogs without resize bounce ([b3d4164](https://github.com/benletchford/systemless/commit/b3d4164f127b685e4d2169aa9d6432d54e0687a5))
* **memory:** validate RecoverHandle master-pointer slots ([b72ea7d](https://github.com/benletchford/systemless/commit/b72ea7ddb50f784fdf49d67f4322b09bd4805f0d))
* **memory:** validate RecoverHandle master-pointer slots ([53c1fa7](https://github.com/benletchford/systemless/commit/53c1fa7afdfa6a018023a017739ad36cc5e1693d))


### Performance Improvements

* **gui:** decode guest framebuffers in Metal ([fca3ecc](https://github.com/benletchford/systemless/commit/fca3ecccf0a88570c5542e4d9eb8faf290f810d1))
* **gui:** decode guest framebuffers in Metal ([76c3925](https://github.com/benletchford/systemless/commit/76c3925496d43097b2cc016b9f2208c3782d14f2))
* **gui:** decouple Metal presentation from emulation ([b165448](https://github.com/benletchford/systemless/commit/b165448b317acae1d3bf51ce94174265dd166d79))
* **gui:** decouple Metal presentation from emulation ([e03f4b0](https://github.com/benletchford/systemless/commit/e03f4b02440b20703644ea86d804cd42dcd948dc)), closes [#23](https://github.com/benletchford/systemless/issues/23)
* **gui:** present macOS frames with Metal ([ec589a8](https://github.com/benletchford/systemless/commit/ec589a825fa6cc7dd0015558317dc70a4af7f9f4))
* **gui:** present macOS frames with Metal ([35cef58](https://github.com/benletchford/systemless/commit/35cef58733c62aa77211188533e2d212129efae5))
* **gui:** skip unchanged Metal guest frames ([812e1a7](https://github.com/benletchford/systemless/commit/812e1a749009da7824d91b65b2dbfb0832e304ec))
* **gui:** skip unchanged Metal guest frames ([d2ab17c](https://github.com/benletchford/systemless/commit/d2ab17c062f7a51859c106f94caddd6a07eabcd0))
* **runner:** fast-forward capped GUI TickCount waits ([7b9b940](https://github.com/benletchford/systemless/commit/7b9b940424f89b8ad70ff9d270fcf082ee642147))
* **runner:** fast-forward capped GUI TickCount waits ([721ab96](https://github.com/benletchford/systemless/commit/721ab96cccf836afa156b787e6d183ef28ac7ea3))

## [0.1.132](https://github.com/benletchford/systemless/compare/v0.1.131...v0.1.132) (2026-07-12)


### Bug Fixes

* honor the system event mask when posting key-up events ([e81e949](https://github.com/benletchford/systemless/commit/e81e949e9bab24b8a806b59992891cf664cf19e7))
* suppress duplicate host keydown events ([38155a2](https://github.com/benletchford/systemless/commit/38155a2b6d3a015ec828588300eb7b58ac14980b))
* terminate exhausted Marathon terminal style insertions ([30f49a4](https://github.com/benletchford/systemless/commit/30f49a4d929c37ecd9e5c4de14b2ead5af420d89))
