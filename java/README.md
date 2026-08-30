# Open Agent Profile for Java

Java 17 support for Open Agent Profile 1.0, at parity with the Python,
TypeScript, Go, and Rust libraries. It includes strict YAML 1.2, JSON, and
Markdown parsing; schema, semantic, and security validation; RFC 8785 identities;
verified inheritance; policy narrowing; normative prompt rendering; conflict-safe
state deltas and retention; atomic persistence; and validation/application CLIs.

Version 1.0.5 is available from [Maven Central](https://central.sonatype.com/artifact/io.github.alexmercedcoder/open-agent-profile/1.0.5).

```xml
<dependency>
  <groupId>io.github.alexmercedcoder</groupId>
  <artifactId>open-agent-profile</artifactId>
  <version>1.0.5</version>
</dependency>
```

```java
ObjectNode profile = OapParser.load(Path.of("reviewer.agent.yaml"));
Oap.ValidationReport report = OapValidator.validate(profile);
if (!report.ok()) throw new IllegalArgumentException(report.errors().toString());
System.out.println(report.digests());
```

Build and run the executable JAR:

```console
JAVA_HOME=/path/to/jdk-17 mvn verify
java -jar target/open-agent-profile-1.0.5-cli.jar validate --digest ../examples/code-reviewer.agent.yaml
java -jar target/open-agent-profile-1.0.5-cli.jar apply ../examples/code-reviewer.agent.yaml ../tests/deltas/learned-conventions.delta.yaml --approve --dry-run
```

Delta operations can only alter `/state`; contract-changing proposals are always
returned for human review and never applied automatically.

Maintainers can produce the complete Maven Central artifact set and GPG
signatures with `mvn -Prelease clean verify`. Publishing additionally requires
a Central Portal user token under server ID `central` in `~/.m2/settings.xml`;
use `mvn -Prelease clean deploy` to upload a manually reviewed deployment.
