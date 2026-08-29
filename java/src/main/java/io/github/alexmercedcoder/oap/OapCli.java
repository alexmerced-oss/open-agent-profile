package io.github.alexmercedcoder.oap;

import java.util.concurrent.Callable;
import picocli.CommandLine;
import picocli.CommandLine.Command;

/** Entry point for the bundled OAP command-line tools. */
@Command(name = "oap", mixinStandardHelpOptions = true, description = "Open Agent Profile 1.0 tools", subcommands = {OapValidateCli.class, OapApplyCli.class})
public final class OapCli implements Callable<Integer> {
  @Override public Integer call() { new CommandLine(this).usage(System.out); return 0; }
  /** Runs the OAP command-line tools. */
  public static void main(String[] arguments) { System.exit(new CommandLine(new OapCli()).execute(arguments)); }
}
