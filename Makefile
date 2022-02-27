TARGET?=x86_64-unknown-uefi
BUILD=build/$(TARGET)
export RUST_TARGET_PATH=$(CURDIR)/targets

include mk/$(TARGET).mk

clean:
	rm -rf build target

$(BUILD)/filesystem:
	mkdir -p $(BUILD)
	rm -f $@.partial
	mkdir $@.partial
	fallocate -l 1MiB $@.partial/kernel
	mv $@.partial $@

$(BUILD)/filesystem.bin: $(BUILD)/filesystem
	mkdir -p $(BUILD)
	rm -f $@.partial
	fallocate -l 255MiB $@.partial
	redoxfs-ar $@.partial $<
	mv $@.partial $@
