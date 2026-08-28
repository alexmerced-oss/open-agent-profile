const { OAP_VERSION, profileDigest } = require("../dist/index.cjs");

if (OAP_VERSION !== "1.0" || !profileDigest({}).startsWith("sha256:")) process.exit(1);
