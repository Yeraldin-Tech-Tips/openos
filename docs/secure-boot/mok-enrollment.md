# Secure Boot (MOK) Development Flow

This is the development path for Secure Boot-enabled OpenOS boot.

## 1. Generate development keypair

```bash
./tools/sign/generate-dev-keys.sh
```

Outputs:

- `keys/dev/MOK.key`
- `keys/dev/MOK.crt`

## 2. Sign EFI loader

```bash
./tools/sign/sign-efi.sh out/bin/openos.efi out/bin/openos-signed.efi keys/dev
```

## 3. Sign kernel payload

```bash
./tools/sign/sign-kernel.sh out/bin/kernel.bin out/bin/kernel.bin.sig keys/dev
```

## 4. Enroll certificate in firmware/MOK manager

1. Convert cert to DER if required by firmware tooling.
2. Import `MOK.crt` into firmware key enrollment UI or shim MOK manager.
3. Reboot and confirm key enrollment.

## 5. Boot validation

- Secure Boot enabled in firmware
- Unsigned `openos.efi` should fail to boot
- Signed `openos-signed.efi` should boot

## Notes

- Development keys are not production keys.
- Protect private keys and rotate if exposed.
