# Issue #1135 (T5): which initial interpolation points PRIMA's BOBYQA uses for
# each rhobeg on DeviceLayout's box [0.6, 1.4]^2 from x0 = [1, 1], on a cheap
# quadratic (maxfun 6, no Palace). It shows that rhobeg = 0.4 is one ulp above
# BOBYQA's limit MINVAL(XU - XL)/2 = 0.39999999999999997 in binary64, so PRIMA
# resets it to MINVAL(XU - XL)/4 = 0.19999999999999998 (DeviceLayout's 0.2 to
# 3e-17) with a warning. Output: results_controls_1135/logs/rhobeg_probe.txt.
#
#   julia benchmarks/transmon_devicelayout_controls/rhobeg_probe.jl   (from the repository root)
import Pkg; Pkg.activate("reference/julia/devicelayout"; io=devnull)
using PRIMA
for rb in (0.1, 0.2, 0.39, 0.399999, 0.39999999999999997, 0.4)
    pts = Vector{Vector{Float64}}()
    f(x) = (push!(pts, copy(x)); (x[1]-0.7)^2 + 2(x[2]-0.69)^2 + 0.3*(x[1]-0.7)*(x[2]-0.69))
    x, info = prima(f, [1.0, 1.0]; ftarget=1e-12, xl=[0.6,0.6], xu=[1.4,1.4], rhobeg=rb, maxfun=6)
    println(rb, " => ", pts[2:5])
end
