---
trigger: model_decision
description: only use when we need to render an avatar .e.g chat message, profile , account , logo
---

### Avatar Component Overview

**Import:**

```rust
use gpui_component::avatar::{Avatar, AvatarGroup};
```

---

### Core Features

* Displays profile images, initials, or placeholders
* Automatic color generation for initials
* Multiple sizes: xsmall, small, medium (default), large, or custom
* Supports grouped display via `AvatarGroup`
* Customizable border, shadow, and rounding

---

### Basic Usage

**Simple Avatar with Image**

```rust
Avatar::new()
    .name("John Doe")
    .src("https://example.com/avatar.jpg")
```

**Avatar with Fallback Initials**

```rust
Avatar::new().name("Jane Smith") // shows "JS" with colored background
```

**Anonymous / Placeholder**

```rust
use gpui_component::IconName;

Avatar::new() // default icon
Avatar::new().placeholder(IconName::Building2)
```

---

### Sizes

```rust
Avatar::new().xsmall()
Avatar::new().small()
Avatar::new() // medium (default)
Avatar::new().large()
Avatar::new().with_size(px(100.)) // custom
```

---

### Custom Styling

```rust
Avatar::new()
    .src("https://example.com/avatar.jpg")
    .with_size(px(100.))
    .border_3()
    .border_color(cx.theme().foreground)
    .shadow_sm()
    .rounded(px(20.))
```

---

### AvatarGroup

**Basic Group**

```rust
AvatarGroup::new()
    .child(Avatar::new().src("https://example.com/user1.jpg"))
    .child(Avatar::new().src("https://example.com/user2.jpg"))
    .child(Avatar::new().name("John Doe"))
```

**Group with Limit**

```rust
AvatarGroup::new()
    .limit(3) // max visible avatars
    .child(Avatar::new().src("user1.jpg"))
    .child(Avatar::new().src("user2.jpg"))
    .child(Avatar::new().src("user3.jpg"))
    .child(Avatar::new().src("user4.jpg")) // hidden
```

**Group with Ellipsis**

```rust
AvatarGroup::new()
    .limit(3)
    .ellipsis() // shows "..." for hidden avatars
```

**Group Sizes**

```rust
AvatarGroup::new().xsmall().children(vec![...])
AvatarGroup::new().small().children(vec![...])
AvatarGroup::new().large().children(vec![...])
```

**Adding Multiple Avatars**

```rust
let avatars = vec![
    Avatar::new().src("user1.jpg"),
    Avatar::new().src("user2.jpg"),
    Avatar::new().name("John Doe"),
];

AvatarGroup::new()
    .children(avatars)
    .limit(5)
    .ellipsis()
```

---

### Example Layouts

**Team Display**

```rust
v_flex()
    .gap_4()
    .child("Development Team")
    .child(
        AvatarGroup::new()
            .limit(4)
            .ellipsis()
            .child(Avatar::new().name("Alice Johnson").src("alice.jpg"))
            .child(Avatar::new().name("Bob Smith").src("bob.jpg"))
            .child(Avatar::new().name("Charlie Brown"))
    )
```

**User Profile Header**

```rust
h_flex()
    .items_center()
    .gap_4()
    .child(
        Avatar::new()
            .src("profile.jpg")
            .name("John Doe")
            .large()
            .border_2()
            .border_color(cx.theme().primary)
    )
    .child(
        v_flex()
            .child("John Doe")
            .child("Software Engineer")
    )
```

**Anonymous User**

```rust
Avatar::new()
    .placeholder(IconName::UserCircle)
    .medium()
```

**Custom Colors**

```rust
Avatar::new().name("Alice")    // automatic color
Avatar::new().name("Bob")      // different color
Avatar::new().name("Charlie")  // another color
```

---

### Summary

* `Avatar` handles images, initials, and placeholders automatically.
* `AvatarGroup` supports overlapping layouts, size control, limits, and ellipsis.
* Fully customizable with borders, shadows, rounding, and explicit sizing.
* Ideal for team displays, profile headers, or anonymous users.
