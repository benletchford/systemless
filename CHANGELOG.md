# Changelog

## [0.7.1](https://github.com/benletchford/systemless/compare/v0.7.0...v0.7.1) (2026-07-27)


### Bug Fixes

* activate queued launches when caller exits ([4fdc8dc](https://github.com/benletchford/systemless/commit/4fdc8dc555571aab5ed916df0b2e135c98f28c51))
* activate queued launches when caller exits ([0b8c40a](https://github.com/benletchford/systemless/commit/0b8c40a1890714df2e05d53b9fed44d48eb7f2c6))
* correct sound doubleback argument order ([f390999](https://github.com/benletchford/systemless/commit/f39099990f19c64e90c297c1d584a900e0d0ef64))
* deliver asynchronous file read completions ([d867cc3](https://github.com/benletchford/systemless/commit/d867cc3d091c9e663483c3558146dadd4d796d2d))
* deliver asynchronous file read completions ([49759a4](https://github.com/benletchford/systemless/commit/49759a4ffb1e2eeea5983f1de3a56a8942ab4e67))
* honor ShieldCursor intersection visibility ([b27a3f3](https://github.com/benletchford/systemless/commit/b27a3f386ab7c9c214c343a0574631d531bb5b11))
* initialize QuickDraw cursor vectors ([27ea994](https://github.com/benletchford/systemless/commit/27ea9940ec80385648715546ccb0bf249d2842e3))
* initialize QuickDraw cursor vectors ([5117ad4](https://github.com/benletchford/systemless/commit/5117ad435dedb15959dd87dc3e9e30518c77fc19))
* initialize shield cursor vector ([ed15013](https://github.com/benletchford/systemless/commit/ed15013469f7912ef5ff88b496ef1c49ed838c2c))
* initialize shield cursor vector ([66faeef](https://github.com/benletchford/systemless/commit/66faeef1fe7af40f6254fa8b552d172c00e2fde6))
* initialize show cursor vector ([2ce71df](https://github.com/benletchford/systemless/commit/2ce71df26d2dd0d1759f58a56711f086deb4a0ac))
* initialize show cursor vector ([0670e85](https://github.com/benletchford/systemless/commit/0670e85fa26924cbd822bde50f4a433e06491829))
* initialize swap mmu trap vector ([016f2d6](https://github.com/benletchford/systemless/commit/016f2d6f4a3ffeae1509a123ca324caedd1c6691))
* initialize swap mmu trap vector ([b3395ae](https://github.com/benletchford/systemless/commit/b3395ae23e5e125000268fd5ec9d3cfb04214587))
* interleave sub-vbl timer callbacks ([0d3a3fe](https://github.com/benletchford/systemless/commit/0d3a3fe2e8e87c4051f71a066f85967ab5c94b25))
* isolate sound callback trampolines ([d1f9317](https://github.com/benletchford/systemless/commit/d1f93176ddf537da142b4afeccd8056999ba5291))
* isolate sound callback trampolines ([3c08c52](https://github.com/benletchford/systemless/commit/3c08c523978e2cb92c0e4e2378750d8976d02e22))
* **memory:** keep application allocations inside the zone boundary ([54d30bc](https://github.com/benletchford/systemless/commit/54d30bcc6a868c20eec077621a77b31d543b66e0))
* **memory:** keep application allocations inside the zone boundary ([76dbf1e](https://github.com/benletchford/systemless/commit/76dbf1e0f5f8c29effd33b8f695b78a2317780c1))
* pace self-reprimed timer tasks ([48494fb](https://github.com/benletchford/systemless/commit/48494fb274f846e23e2459a69b245c881bb6773a))
* preserve concurrent timer callbacks ([69e6402](https://github.com/benletchford/systemless/commit/69e64027a54bbef4808c6791023d869ff082ec55))
* preserve sub-vbl timer deadlines ([284495f](https://github.com/benletchford/systemless/commit/284495f9361699a4f3a8cb26d5cd263143d7ed4c))
* prioritize overdue timer callbacks ([95fad8f](https://github.com/benletchford/systemless/commit/95fad8fed3ef017e74469e7adc9b0ddbce6d7a28))
* **quickdraw:** support packed indexed CopyBits sources ([7803647](https://github.com/benletchford/systemless/commit/78036473dd4dd15235f57450fb0bebe2843938bd))
* **quickdraw:** support packed indexed CopyBits sources ([d211fa4](https://github.com/benletchford/systemless/commit/d211fa4202d73d883c1ab991bb16a38549d76ecf))
* resolve menu definition procedure handles ([25f7829](https://github.com/benletchford/systemless/commit/25f7829412190352ff4dbac737b399fd0b145d6e))
* stabilize centered fullscreen margins ([377ef59](https://github.com/benletchford/systemless/commit/377ef59cd1797ddb7288b437a55866710b878c9b))
* stabilize centered fullscreen margins ([f5fffd0](https://github.com/benletchford/systemless/commit/f5fffd0696ae6262485553a3ca9ae23bf9c7293e))
* **window:** expose the legacy window manager port layout ([c4d7b28](https://github.com/benletchford/systemless/commit/c4d7b2885b85ae9db154fe595844a48427a64145))
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
