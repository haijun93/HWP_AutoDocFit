using System;
using System.IO;
using System.Text;

internal static class RustLauncherArgsChild
{
    private static int Main(string[] args)
    {
        if (args.Length >= 3 && args[0] == "--self-test" && args[1] == "--self-test-report")
        {
            File.WriteAllText(args[2], "{\"ok\":true}", new UTF8Encoding(false));
            return 0;
        }
        string output = Environment.GetEnvironmentVariable("DOCFIT_ARGS_REPORT");
        File.WriteAllLines(output, args, new UTF8Encoding(false));
        return 0;
    }
}
