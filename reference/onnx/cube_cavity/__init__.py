"""ONNX cube-cavity assembly graph (Epic #88, Phase F.2).

See :mod:`reference.onnx.cube_cavity.assembly_graph` for the static-graph
end-to-end builder, and :mod:`reference.onnx.cube_cavity.gen_cube_cavity_reduced`
for the driver that runs the graph through onnxruntime and emits the
schema-v1 sidecar.

This package is the runtime payload for Phase F.2; it derives directly
from the Phase F.1 audit in ``reference/onnx/audit/`` and uses raw
``onnx.helper`` for IR-level transparency (NOT ``onnxscript``).
"""
