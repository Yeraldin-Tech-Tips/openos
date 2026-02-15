# Kernel Driver Implementation Roadmap

This document tracks implementation status for scaffolded kernel drivers and defines minimum viable bring-up milestones.

## Milestone Definitions (Minimum Viable)

- [ ] **M1: PCI/PS2/USB detection**
  - Enumerate PCI devices and match supported IDs for Intel Ethernet/Wi-Fi.
  - Detect legacy PS/2 controller and USB controller presence for HID.
- [ ] **M2: BAR/resource mapping**
  - Validate BAR type/size and map MMIO/PIO resources for network drivers.
  - Reserve/initialize controller I/O regions used by HID paths.
- [ ] **M3: Init sequence**
  - Reset hardware, configure interrupts/queues/rings, and transition device to ready state.
  - Integrate deterministic init logs and explicit failure reasons.
- [ ] **M4: Basic send/recv/input path**
  - Ethernet: one-frame TX + RX path.
  - Wi-Fi: baseline station-mode frame path after firmware bring-up.
  - HID: keyboard/mouse event ingest from controller to kernel input pipeline.

## Stub Driver Checklist Mapping

### Intel Ethernet (`kernel/src/drivers/ethernet_intel.rs`)

- [ ] [DRV-ETH-001] M1 detection (PCI ID scan)
- [ ] [DRV-ETH-002] M2 BAR/resource mapping
- [ ] [DRV-ETH-003] M3 init sequence
- [ ] [DRV-ETH-004] M4 basic send/recv path

### Intel Wi-Fi (`kernel/src/drivers/wifi_intel.rs`)

- [ ] [DRV-WIFI-001] M1 detection (PCI ID scan)
- [ ] [DRV-WIFI-002] M2 BAR/resource mapping
- [ ] [DRV-WIFI-003] M3 init sequence + firmware load
- [ ] [DRV-WIFI-004] M4 basic send/recv path

### HID stack (`kernel/src/drivers/hid.rs`)

- [ ] [DRV-HID-001] M1 PS/2 + USB controller detection
- [ ] [DRV-HID-002] M2 resource wiring (I/O regions, IRQ routing)
- [ ] [DRV-HID-003] M3 init sequence
- [ ] [DRV-HID-004] M4 basic input event path
