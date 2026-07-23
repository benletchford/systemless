# Changelog

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
