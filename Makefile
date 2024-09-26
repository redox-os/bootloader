TARGET?=x86_64-unknown-uefi
SOURCE:=$(dir $(realpath $(lastword $(MAKEFILE_LIST))))
BUILD:=$(CURDIR)
export RUST_TARGET_PATH?=$(SOURCE)/targets


include $(SOURCE)/mk/$(TARGET).mk

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
	fallocate -l 254MiB $@.partial
	redoxfs-ar $@.partial $<
	mv $@.partial $@
