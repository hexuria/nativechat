# Credential Exchange: importing from the Passwords app

Apple's Passwords app (macOS 26) hands passwords, passkeys and authenticator codes to
another app **in memory**, through the FIDO Credential Exchange. It never writes a file.
The app on the receiving end must be a **credential provider**: a signed app with an
app-extension target and the AutoFill Credential Provider entitlement. The ad-hoc dev
build cannot be one, so this directory holds the pieces ready for the signed build, and
nothing here is compiled into `nativechat` today.

What is here:

- `ImportReceiver.swift` — the receiver. The OS launches the app with an `NSUserActivity`
  of type `ASCredentialExchangeActivity` whose `userInfo` carries the import token; the
  receiver calls `ASCredentialImportManager().importCredentials(token:)` and turns the
  result into the same `ImportedItem` rows the file importers produce (JSON over a C ABI,
  see `shim.h`). Secrets stay in memory; nothing is logged.
- `shim.h` / `shim.swift` — the C ABI the Rust app calls: `nc_credential_exchange_import`
  (token in, JSON out, freed with `nc_credential_exchange_free`).
- `Extension-Info.plist` — the keys the **extension** target needs:
  `NSExtension > NSExtensionAttributes > ASCredentialProviderExtensionCapabilities >
  SupportsCredentialExchange = YES` and `SupportedCredentialExchangeVersions = ["1.0"]`.
- `App-Info.plist.fragment` — the keys the **app** target needs: `NSUserActivityTypes`
  with `ASCredentialExchangeActivityType`.

What the signed build needs, in order:

1. A Developer ID (or App Store) signing identity and a provisioning profile that grants
   `com.apple.developer.authentication-services.autofill-credential-provider` to **both**
   the app and the extension (Xcode → Signing & Capabilities → AutoFill Credential
   Provider). Apple grants this capability per app; ask when the identity exists.
2. An app-extension target of type Credential Provider (`ASCredentialProviderViewController`
   subclass, can stay minimal) with `Extension-Info.plist`'s keys.
3. `App-Info.plist.fragment` merged into the app's Info.plist (`cargo bundle` reads
   `[package.metadata.bundle]`; add the key there).
4. The Swift files built into a static library with a C ABI (`swiftc -emit-library
   -static`), linked by `build.rs`, and `nc_credential_exchange_import` called from
   `src/site_login/importers/` when the app receives the activity (an `NSApplication`
   delegate hook: `application(_:continue:restorationHandler:)`).
5. Then, in the Passwords app: File → Export → to another app → choose NativeChat.

Type-check the Swift with the SDK on this Mac:

    swiftc -typecheck -sdk "$(xcrun --sdk macosx --show-sdk-path)" -target arm64-apple-macos26.0 \
      macos/CredentialExchange/ImportReceiver.swift macos/CredentialExchange/shim.swift
