# Kubuno Office — mobile apps

Reserved for the mobile clients of the Office module, organised like the desktop app (`../desktop`):

```
mobile/
  common/    the complete mobile app, shared by Android and iOS
  android/   only what Android does differently, and its entry point
  ios/       only what iOS does differently, and its entry point
```

Today the Android Documents app lives in the `kubuno/mobile` repository (`android/app-docs`); it moves here when the mobile
apps follow the one-repository-per-module organisation. The document engine of `../common/core`
(`kubuno-office-docs-core`) is written to be shared with it.
