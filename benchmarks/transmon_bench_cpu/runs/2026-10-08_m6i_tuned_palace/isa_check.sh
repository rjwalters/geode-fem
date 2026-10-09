#!/bin/bash
# ISA evidence for the binaries of the image: per file, instructions touching
# any ymm register (AVX/AVX2: none at -march=x86-64-v2), ymm16-31 / zmm or an
# opmask register k1-k7 (AVX-512 only: EVEX encoding). GCC at
# -march=icelake-server prefers 256-bit vectors, so zmm stays near 0 there.
for f in $(find /opt/palace/bin /opt/palace/lib /opt/palace/lib64 /opt/openblas/lib -maxdepth 1 -type f \( -name "*.bin" -o -name "*.so*" \) | sort); do
  case "$(basename "$f")" in
    palace-*.bin|libpalace.so*|libmfem.so*|libpetsc.so*|libslepc.so*|libHYPRE*.so*|libsuperlu_dist.so*|libstrumpack.so*|libdmumps.so*|libzmumps.so*|libscalapack.so*|libceed.so*|libxsmm.so*|libopenblas*.so*) ;;
    *) continue ;;
  esac
  d=$(objdump -d --no-show-raw-insn "$f")
  echo "$(basename "$f") ymm=$(grep -c "%ymm" <<<"$d") evex_regs=$(grep -cE "%(ymm(1[6-9]|2[0-9]|3[01])|xmm(1[6-9]|2[0-9]|3[01])|zmm[0-9]+)" <<<"$d") opmask=$(grep -cE "%k[1-7]" <<<"$d") zmm=$(grep -c "%zmm" <<<"$d")"
done
