# Issue #1129 (T1): DeviceLayout.jl's PRIMA re-optimization of the
# SingleTransmon example, re-run with this repository's Palace build.
#
# The published count (9 Palace runs to err < 1e-4) came from DeviceLayout's
# own Palace build. This script repeats `run_optimization` exactly — the same
# `prima(errfunc, [1.0, 1.0]; ftarget = reltol^2, xl = [0.6, 0.6],
# xu = [1.4, 1.4], rhobeg = 0.2)` call, which with bounds only dispatches to
# BOBYQA, and the same objective sum((f - t)^2 / t^2) / 2 with
# cap_length = (1 / x1^2) 620 um and total_length = (1 / x2) 5000 um — but
# every evaluation is a Palace run on palace:cpu-1043 (fba6a5b, the #1092
# Dockerfile) at DeviceLayout's discretization: its mesh_order 2 mesh, its
# configfile(sm; solver_order = 2) verbatim, the same cells as the #1123
# Newton controls (driver.py eval). The first point, x = [1, 1], is the
# initial design, whose run is the committed part-1 cell
# results_crosscheck_1092/dl_initial (run 1, as #1111 and #1123 count it).
#
#   julia --project=reference/julia/devicelayout \
#       benchmarks/transmon_devicelayout_controls/prima_driver.jl \
#       --scratch <dir> --worker <name> [--np 16] [--remote work1129ctl] \
#       [--replay <cells dir>] [--max-runs 25] [--rhobeg 0.2] [--prefix pr]
#
# --rhobeg (#1135, T5) changes only PRIMA's initial trust radius; the default
# 0.2 is DeviceLayout's. --prefix names the cells dl_<prefix><n> (default
# "pr", the #1129 cells); #1135 ran rhobeg 0.1 as "pa" and 0.4 as "pb".
#
# Without --replay every new point goes to the worker through
#   python3 -I benchmarks/transmon_devicelayout_controls/driver.py eval ...
# and is logged to <scratch>/prima_log.jsonl. With --replay <cells dir> no
# Palace run is made: each point PRIMA asks for is read from <cells
# dir>/dl_pr<n> (checked against its generator report), so re-running the
# optimizer on the committed cells reproduces the trajectory and the count,
# or fails loudly. Run from the repository root.

import Pkg
Pkg.activate(joinpath(@__DIR__, "..", "..", "reference", "julia", "devicelayout"))

using PRIMA
import JSON

const TARGETS = [3.0, 4.0]
const RELTOL = 1e-2
const INITIAL_CELL = "reference/fixtures/transmon_palace/results_crosscheck_1092/dl_initial"

function parse_args(args)
    o = Dict{String,Any}(
        "scratch" => "",
        "worker" => "w1129",
        "np" => "16",
        "remote" => "work1129ctl",
        "replay" => "",
        "max-runs" => "25",
        "rhobeg" => "0.2",
        "prefix" => "pr",
    )
    i = 1
    while i <= length(args)
        k = args[i]
        startswith(k, "--") && haskey(o, k[3:end]) && i < length(args) || error("bad argument $k")
        o[k[3:end]] = args[i + 1]
        i += 2
    end
    isempty(o["scratch"]) && error("--scratch is required")
    return o
end

"""The two lowest modes with junction participation |p| >= 1e-6 (the
port-sheet filter of #1106/#1111/#1123; with N = 2 it keeps both, as
DeviceLayout's `freq[1:2]` does)."""
function read_cell(dir)
    parse_col(path, col) = [
        parse(Float64, strip(split(l, ",")[col == :last ? end : col])) for
        l in readlines(path)[2:end] if !isempty(strip(l))
    ]
    f = parse_col(joinpath(dir, "eig.csv"), 2)
    p = abs.(parse_col(joinpath(dir, "port-EPR.csv"), :last))
    kept = [fi for (fi, pi) in zip(f, p) if pi >= 1e-6]
    return kept[1:2]
end

function report_design(dir)
    t = read(joinpath(dir, "generator_report.txt"), String)
    c = parse(Float64, match(r"cap_length:\s*(\S+) um", t).captures[1])
    L = parse(Float64, match(r"total_length:\s*(\S+) um", t).captures[1])
    return c, L
end

errf(f) = sum((f .- TARGETS) .^ 2 ./ TARGETS .^ 2) / 2

function main(args)
    o = parse_args(args)
    scratch = o["scratch"]
    mkpath(scratch)
    replay = o["replay"]
    max_runs = parse(Int, o["max-runs"])
    rhobeg = parse(Float64, o["rhobeg"])
    prefix = o["prefix"]
    logpath = joinpath(scratch, isempty(replay) ? "prima_log.jsonl" : "prima_replay_log.jsonl")
    isfile(logpath) && rm(logpath)
    n = Ref(0)
    function errfunc(x, p = ())
        n[] += 1
        # The transform of DeviceLayout's frequency_targeting_errfunc, in um.
        c = (1 / x[1]^2) * 620.0
        L = (1 / x[2]) * 5000.0
        if x == [1.0, 1.0]
            cell, src = "dl_initial", INITIAL_CELL
            f = read_cell(src)
        elseif !isempty(replay)
            cell = "dl_$(prefix)$(n[])"
            src = joinpath(replay, cell)
            isdir(src) || error("replay: PRIMA asked for run $(n[]) at x = $x, but $src does not exist")
            cr, Lr = report_design(src)
            cr == c && Lr == L ||
                error("replay: run $(n[]) is x = $x -> ($c, $L) um, but $src was meshed at ($cr, $Lr)")
            f = read_cell(src)
        else
            n[] > max_runs && error("more than $max_runs Palace runs: stopping (budget cap)")
            cell = "dl_$(prefix)$(n[])"
            cmd = `python3 -I benchmarks/transmon_devicelayout_controls/driver.py eval
                   --scratch $scratch --worker $(o["worker"]) --np $(o["np"]) --remote $(o["remote"])
                   --c $(repr(c)) --L $(repr(L)) --tag $(prefix)$(n[])`
            out = read(cmd, String)
            m = match(r"EVAL (\{.*\})", out)
            m === nothing && error("driver.py eval gave no result:\n$out")
            rec = JSON.parse(m.captures[1])
            f = Float64.(rec["f"])
            src = "worker"
        end
        e = errf(f)
        rec = Dict(
            "run" => n[],
            "x" => [repr(x[1]), repr(x[2])],
            "c_um" => repr(c),
            "L_um" => repr(L),
            "cell" => cell,
            "f_ghz" => f,
            "err" => e,
            "below_criterion" => e < RELTOL^2,
        )
        open(logpath, "a") do io
            println(io, JSON.json(rec))
        end
        println(JSON.json(rec))
        flush(stdout)
        return e
    end
    x, info = prima(errfunc, [1.0, 1.0]; ftarget = RELTOL^2, xl = [0.6, 0.6], xu = [1.4, 1.4], rhobeg = rhobeg)
    summary = Dict(
        "nf" => info.nf,
        "status" => string(info.status),
        "reason" => PRIMA.reason(info),
        "fx" => info.fx,
        "x" => [repr(x[1]), repr(x[2])],
        "c_um" => repr((1 / x[1]^2) * 620.0),
        "L_um" => repr((1 / x[2]) * 5000.0),
        "prima_version" => string(pkgversion(PRIMA)),
        "replay" => !isempty(replay),
        "rhobeg" => rhobeg,
        "prefix" => prefix,
    )
    open(joinpath(scratch, isempty(replay) ? "prima_summary.json" : "prima_replay_summary.json"), "w") do io
        JSON.print(io, summary, 2)
    end
    println("SUMMARY ", JSON.json(summary))
end

main(ARGS)
