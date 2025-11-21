---
trigger: model_decision
description: When an OTP UI / Component would be built e.g. verify code page or an OTP page
---

# **OtpInput**

## **Import**

```rust
use gpui_component::input::{OtpInput, OtpState};
```

---

# **Core Pattern (Use This 90% of the Time)**

```rust
let otp_state = cx.new(|cx| OtpState::new(6, window, cx));

OtpInput::new(&otp_state)
```

---

# **Set a Default Value**

```rust
let otp_state = cx.new(|cx|
    OtpState::new(6, window, cx)
        .default_value("123456")
);
```

---

# **Masked OTP (most apps want this)**

```rust
let otp_state = cx.new(|cx|
    OtpState::new(6, window, cx)
        .masked(true)
);
```

---

# **Sizing**

```rust
OtpInput::new(&otp_state).small()
OtpInput::new(&otp_state)         // medium (default)
OtpInput::new(&otp_state).large()
OtpInput::new(&otp_state).with_size(px(55.))
```

---

# **Groups**

```rust
OtpInput::new(&otp_state).groups(1) // no grouping
OtpInput::new(&otp_state).groups(2) // default
OtpInput::new(&otp_state).groups(3)
```

---

# **Disabled**

```rust
OtpInput::new(&otp_state).disabled(true)
```

---

# **Different Length Codes**

```rust
OtpInput::new(&cx.new(|cx| OtpState::new(4, window, cx))) // 4-digit
OtpInput::new(&cx.new(|cx| OtpState::new(6, window, cx))) // 6-digit (SMS)
OtpInput::new(&cx.new(|cx| OtpState::new(8, window, cx))).groups(2) // 8-digit
```

---

# **OTP Events (The only event handler you truly need)**

## **Auto-submit on complete**

```rust
cx.subscribe(&otp_state, |this, state, event: &InputEvent, cx| {
    if let InputEvent::Change = event {
        let code = state.read(cx).value();
        if code.len() == 6 {
            this.verify_otp(&code, cx);
        }
    }
});
```

## Optional: focus / blur

```rust
InputEvent::Focus => {}
InputEvent::Blur => {}
```

---

# **Programmatic Control**

## **Set value**

```rust
otp_state.update(cx, |state, cx| {
    state.set_value("123456", window, cx);
});
```

## **Mask / unmask**

```rust
otp_state.update(cx, |s, cx| s.set_masked(true, window, cx));
```

## **Focus**

```rust
otp_state.update(cx, |s, cx| s.focus(window, cx));
```

## **Get value**

```rust
let val = otp_state.read(cx).value();
```

---

# **Behavior You Get For Free**

* Only digits allowed
* Auto-advance
* Backspace jumps backwards
* Only accepts fixed length
* Emits `Change` when complete
* Keyboard navigation works automatically
* Masking works like password fields
* Disabled state visuals

You don’t implement any of this — it’s built in.

---

# **Best Real-World Patterns**

## **1. Auto-submit OTP (always use this)**

```rust
if code.len() == 6 {
    this.submit(&code, cx);
}
```

## **2. Clear on focus**

```rust
InputEvent::Focus => {
    state.update(cx, |s, cx| s.set_value("", window, cx));
}
```

## **3. PIN Entry with lockout**

```rust
OtpInput::new(&pin_state).groups(1).disabled(is_locked)
```

## **4. TOTP (Authenticator)**

```rust
OtpInput::new(&otp_state).groups(3).masked(true)
```

## **5. SMS Login**

```rust
OtpInput::new(&otp_state).large()
```

---

# **API Cheat Sheet**

## **OtpState**

* `new(len, window, cx)`
* `default_value(str)`
* `masked(bool)`
* `set_value(str, window, cx)`
* `value()`
* `set_masked(bool, window, cx)`
* `focus(window, cx)`
* `focus_handle(cx)`

## **OtpInput**

* `new(state)`
* `groups(n)`
* `disabled(bool)`
* `small()`
* `large()`
* `with_size(px)`

---

# **Opinionated Recommendations**

* Always **mask** OTPs unless it’s a PIN screen.
* Use **groups(1)** for PIN, **groups(2)** for SMS, **groups(3)** for authenticator.
* Use **large()** when OTP is the main action on the screen (login flows).
* Auto-submit the moment the code completes.
* Clear the field on focus to prevent stuck ghost values.
