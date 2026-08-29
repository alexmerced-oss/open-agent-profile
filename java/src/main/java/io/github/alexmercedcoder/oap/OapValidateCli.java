package io.github.alexmercedcoder.oap;

import java.nio.file.Path;
import java.util.List;
import java.util.concurrent.Callable;
import picocli.CommandLine.Command;
import picocli.CommandLine.Option;
import picocli.CommandLine.Parameters;

/** Validates one or more profile or delta documents. */
@Command(name = "validate", aliases = "oap-validate", mixinStandardHelpOptions = true, description = "Validate OAP profiles and deltas")
public final class OapValidateCli implements Callable<Integer> {
  @Parameters(arity = "1..*", paramLabel = "FILE") private List<Path> paths;
  @Option(names = "--strict", description = "Treat warnings as failures") private boolean strict;
  @Option(names = "--digest", description = "Print profile and specification digests") private boolean digest;
  @Override public Integer call() {
    boolean failed = false;
    for (Path path : paths) { Oap.ValidationReport report = OapValidator.validate(path); report.errors().forEach(issue -> System.err.println("error: " + pointer(issue) + ": " + issue.message())); report.warnings().forEach(issue -> System.err.println("warn: " + pointer(issue) + ": " + issue.message()));
      if (!report.ok() || (strict && !report.warnings().isEmpty())) failed = true; else System.out.println(path + ": valid " + report.kind()); if (digest && report.digests() != null) System.out.println("  profile " + report.digests().profile() + "\n  spec    " + report.digests().spec()); }
    return failed ? 1 : 0;
  }
  private static String pointer(Oap.Issue issue) { return issue.pointer().isEmpty() ? "<root>" : issue.pointer(); }
}
