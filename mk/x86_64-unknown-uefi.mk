export PARTED?=parted
export QEMU?=qemu-system-x86_64

all: $(BUILD)/bootloader.efi

$(BUILD)/bootloader.efi: $(SOURCE)/Cargo.toml $(SOURCE)/Cargo.lock $(shell find $(SOURCE)/src -type f)
	mkdir -p "$(BUILD)"
	env RUSTFLAGS="-C soft-float" \
	cargo rustc \
		--manifest-path="$<" \
		-Z build-std=core,alloc \
		-Z build-std-features=compiler-builtins-mem \
		--target $(TARGET) \
		--bin bootloader \
		--release \
		-- \
		--emit link="$@"

$(BUILD)/bootloader-live.efi: $(SOURCE)/Cargo.toml $(SOURCE)/Cargo.lock $(shell find $(SOURCE)/src -type f)
	mkdir -p $(BUILD)
	cd "$(SOURCE)"
	env RUSTFLAGS="-C soft-float" \
	cargo rustc \
		--manifest-path="$<" \
		-Z build-std=core,alloc \
		-Z build-std-features=compiler-builtins-mem \
		--target $(TARGET) \
		--bin bootloader \
		--release \
		--features live \
		-- \
		--emit link="$@"

$(BUILD)/esp.bin: $(BUILD)/bootloader.efi
	rm -f "$@.partial"
	fallocate -l 1MiB $@.partial
	mkfs.vfat "$@.partial"
	mmd -i "$@.partial" efi
	mmd -i "$@.partial" efi/boot
	mcopy -i "$@.partial" "$<" ::efi/boot/bootx64.efi
	mv $"@.partial" "$@"

$(BUILD)/harddrive.bin: $(BUILD)/esp.bin $(BUILD)/filesystem.bin
	rm -f "$@.partial"
	fallocate -l 320MiB "$@.partial"
	$(PARTED) -s -a minimal "$@.partial" mklabel gpt
	$(PARTED) -s -a minimal "$@.partial" mkpart ESP FAT32 1MiB 2MiB
	$(PARTED) -s -a minimal "$@.partial" mkpart REDOXFS 2MiB 100%
	$(PARTED) -s -a minimal "$@.partial" toggle 1 boot
	dd if="$(BUILD)/esp.bin" of="$@.partial" bs=1MiB seek=1 conv=notrunc
	dd if="$(BUILD)/filesystem.bin" of="$@.partial" bs=1MiB seek=2 conv=notrunc
	mv "$@.partial" "$@"

$(BUILD)/firmware.rom: /usr/share/OVMF/OVMF_CODE.fd
	cp "$<" "$@"

qemu: $(BUILD)/harddrive.bin $(BUILD)/firmware.rom
	$(QEMU) \
		-d cpu_reset \
		-no-reboot \
		-smp 4 -m 2048 \
		-chardev stdio,id=debug,signal=off,mux=on \
		-serial chardev:debug \
		-mon chardev=debug \
		-machine q35 \
		-net none \
		-enable-kvm \
		-cpu host \
		-bios "$(BUILD)/firmware.rom" \
		-drive file="$<",format=raw
