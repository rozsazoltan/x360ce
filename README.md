# x360ce

<p align="center">
  <img src="assets/x360ce.png" width="160" alt="x360ce controller icon">
</p>

`x360ce` lets a physical Windows controller behave like an Xbox 360 controller. Select a device, map its inputs visually, then keep the app in the tray while it forwards input to the virtual controller.

The project follows the familiar workflow of the original [`x360ce/x360ce`](https://github.com/x360ce/x360ce), with a focused Windows interface, live input preview, per-controller profiles, and portable settings.

- [Get started](#get-started)
- [Mapping](#mapping)
- [Tray mode](#tray-mode)
- [Profiles](#profiles)
- [Troubleshooting](#troubleshooting)
- [License](#license)

## Get started

> [!IMPORTANT]
> x360ce requires 64-bit Windows 10 or Windows 11 and the ViGEmBus driver. The app can open the bundled driver installer when ViGEmBus is missing.

1. Download the latest `x360ce.exe` from Releases.
2. Move it to a permanent, writable folder.
3. Start x360ce and install ViGEmBus when prompted.
4. Restart x360ce, connect the controller, then select it under **Input controller**.
5. Map the controls and move x360ce to the tray.

Settings and profiles are saved beside the executable, so the application remains portable.

## Mapping

Choose a control on the virtual Xbox 360 layout or in the mapping list. Double-click it, then press a physical button or move an axis. Press `Esc` to cancel learning.

| State | Meaning |
|---|---|
| White | Mapped |
| Green | Currently active |
| Yellow with `!` | Not mapped |
| Blue | Waiting for learned input |

Buttons, triggers, D-pad directions, stick axes, and stick clicks can be mapped independently. A missing mapping is valid when the physical controller has fewer controls.

For axes, x360ce detects positive and negative movement separately. Use **Source direction** or **Invert output** when an axis moves the wrong way. Deadzone and saturation controls can refine stick behavior, while **Centered/split source axis** supports controllers that expose both triggers through one shared axis.

Use the remove button beside a mapping to clear only that control. The **Live physical input** panel shows raw axes, buttons, and hats from the selected device.

## Tray mode

Closing the window hides x360ce instead of stopping it. Use **Quit x360ce** from the tray menu to remove the virtual controller completely.

When **Forward only in tray** is enabled, physical input remains visible while configuring the app, but virtual output stays paused until x360ce is hidden. This prevents test presses from controlling Windows, games, or other applications during mapping.

Automatic tray mode can hide the window after an idle period. Mouse, keyboard, or controller activity cancels its countdown.

x360ce also suppresses Guide/Nexus output and silently handles Windows Game Bar protocol requests, preventing Xbox Game Bar or its missing-application dialog from appearing.

## Profiles

Each controller receives its own profile. Use **Export** to save the selected mapping as JSON and **Import** to restore or share it.

Portable application data is stored here:

```text
.x360ce-data\config.json
```

Back up `x360ce.exe` together with `.x360ce-data` when moving the application to another computer or folder.

## Troubleshooting

<details>
<summary>Virtual Xbox 360 controller does not appear</summary>

Install ViGEmBus from x360ce, restart the app, and confirm that emulation is enabled. When **Forward only in tray** is active, hide x360ce before testing. Games already running may need to be restarted.

</details>

<details>
<summary>Physical controller is missing</summary>

Reconnect the controller, press **Refresh**, or try another USB or Bluetooth connection. Close other controller tools that may hold exclusive device access.

</details>

<details>
<summary>Axis or trigger moves incorrectly</summary>

Change **Source direction**, enable **Invert output**, or use **Centered/split source axis** for shared trigger axes.

</details>

<details>
<summary>Inputs appear twice in a game</summary>

Windows still exposes the physical controller while x360ce creates a virtual one. Games that read both devices may require a separate physical-device hiding solution such as HidHide.

</details>

<details>
<summary>Xbox Game Bar dialog still appears</summary>

Quit every running x360ce instance, start the latest build once so its silent protocol handlers are registered, then hide it to the tray and test the Guide/Nexus button again.

</details>

<details>
<summary>Settings are not saved</summary>

Keep x360ce in a folder writable by the current user. Avoid protected locations such as `C:\Program Files` unless permissions are configured manually.

</details>

## License

x360ce is open source under the [GNU Affero General Public License v3.0 or later](LICENSE). Third-party notices are available in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Development, testing, and contribution documentation lives in [CONTRIBUTING.md](CONTRIBUTING.md).
