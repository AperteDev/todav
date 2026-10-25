# COPR's "make srpm" build method: the source tarball from this checkout, the crates it needs
# vendored next to it (COPR builds without network), then the source RPM. COPR passes outdir
# and spec; locally:
#   make -f .copr/Makefile srpm outdir=$PWD/target/rpm spec=$PWD/todav/linux/rpm/todav.spec
spec ?= $(CURDIR)/todav/linux/rpm/todav.spec
outdir ?= $(CURDIR)/target/rpm
top := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/..)

srpm:
	command -v cargo git rpmbuild xz >/dev/null || dnf -y install cargo git rpm-build xz
	version=$$(sed -n 's/^Version: *//p' $(spec)); \
	work=$$(mktemp -d); \
	set -e; \
	git -C $(top) config --global --add safe.directory $(top) 2>/dev/null || true; \
	mkdir -p $(outdir); \
	git -C $(top) archive --prefix=todav-$$version/ -o $(outdir)/todav-$$version.tar.gz HEAD; \
	cd $(top) && cargo vendor --locked --versioned-dirs $$work/vendor >/dev/null; \
	tar -C $$work -cJf $(outdir)/todav-$$version-vendor.tar.xz vendor; \
	rm -rf $$work; \
	rpmbuild -bs --define "_sourcedir $(outdir)" --define "_srcrpmdir $(outdir)" $(spec)

.PHONY: srpm
