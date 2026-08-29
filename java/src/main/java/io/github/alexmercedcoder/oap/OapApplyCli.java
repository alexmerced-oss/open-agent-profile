package io.github.alexmercedcoder.oap;

import com.fasterxml.jackson.databind.node.ObjectNode;
import java.nio.file.Path;
import java.util.concurrent.Callable;
import picocli.CommandLine.Command;
import picocli.CommandLine.Option;
import picocli.CommandLine.Parameters;

/** Validates and applies an OAP state delta. */
@Command(name = "apply", aliases = "oap-apply", mixinStandardHelpOptions = true, description = "Apply an OAP state delta")
public final class OapApplyCli implements Callable<Integer> {
  @Parameters(index = "0", paramLabel = "PROFILE") private Path profilePath;
  @Parameters(index = "1", paramLabel = "DELTA") private Path deltaPath;
  @Option(names = "--approve", description = "Approve propose-mode writeback") private boolean approved;
  @Option(names = "--dry-run", description = "Print instead of replacing the profile") private boolean dryRun;
  @Option(names = "--actor", paramLabel = "NAME", description = "Actor recorded in history") private String actor;
  @Override public Integer call() throws Exception {
    ObjectNode profile = OapParser.load(profilePath); ObjectNode delta = OapParser.load(deltaPath);
    for (ObjectNode document : new ObjectNode[] {profile, delta}) { Oap.ValidationReport report = OapValidator.validate(document); if (!report.ok()) { report.errors().forEach(issue -> System.err.println((issue.pointer().isEmpty() ? "<root>" : issue.pointer()) + ": " + issue.message())); return 1; } }
    Oap.DeltaApplication result = OapDelta.apply(profile, delta, new OapDelta.ApplyOptions(approved, actor, null)); String text = OapDelta.serialize(result.profile(), OapParser.format(profilePath)); result.warnings().forEach(warning -> System.err.println("warn: " + warning)); if (!result.pendingProposals().isEmpty()) System.err.println(result.pendingProposals().size() + " proposal(s) require human review and were not applied"); if (dryRun) System.out.print(text); else OapDelta.writeAtomically(profilePath, text); System.err.println((dryRun ? "would write" : "wrote") + " revision " + result.profile().path("metadata").path("revision").asLong()); return 0;
  }
}
