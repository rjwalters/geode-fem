# URLs and absolute paths — all skipped by the lint

These references should produce ZERO findings because they fall outside the
lint's scope:

![a](https://example.com/x.png)

![b](http://example.com/y.png)

![c](/abs/path.png)

![d](data:image/png;base64,iVBORw0KGgo=)

<img src="https://cdn.example.com/asset.png" alt="cdn-img">
