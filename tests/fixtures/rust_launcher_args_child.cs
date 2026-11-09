using System;
using System.IO;
using System.Text;

internal static class RustLauncherArgsChild
{
    private static int Main(string[] args)
    {
        string output = Environment.GetEnvironmentVariable("DOCFIT_ARGS_REPORT");
        File.WriteAllLines(output, args, new UTF8Encoding(false));
        return 0;
    }
}
