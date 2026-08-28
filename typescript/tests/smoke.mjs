import { OAP_VERSION, profileDigest } from "../dist/index.js";

if (OAP_VERSION !== "1.0" || !profileDigest({}).startsWith("sha256:")) process.exit(1);
