# pinned-symlink v3 (highest N, overridden by symlink)

Highest-numbered version dir but NOT what the resolver returns when
the operator has pinned `.latest -> pinned-symlink.2`. The symlink
takes precedence per the load-bearing AC from issue #288.
