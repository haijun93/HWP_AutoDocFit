using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Text;
using System.Threading;
using System.Windows.Forms;

internal static class RealGuiChild
{
    [STAThread]
    private static int Main(string[] args)
    {
        string mutexName = Environment.GetEnvironmentVariable("DOCFIT_LAUNCHER_MUTEX_NAME");
        string eventName = Environment.GetEnvironmentVariable("DOCFIT_LAUNCHER_HANDOFF_EVENT");
        string version = Environment.GetEnvironmentVariable("DOCFIT_TEST_VERSION") ?? "9.99.0";
        string readyFile = Environment.GetEnvironmentVariable("DOCFIT_READY_FILE");

        var form = new Form
        {
            Text = "한글편집 후처리 (HWP_AutoDocFit) v" + version,
            Width = 600,
            Height = 400,
            StartPosition = FormStartPosition.CenterScreen
        };

        form.Shown += (s, e) =>
        {
            if (!string.IsNullOrEmpty(readyFile))
            {
                try
                {
                    File.WriteAllText(readyFile, "{\"shown\":true}", Encoding.UTF8);
                }
                catch { }
            }
        };

        if (!string.IsNullOrEmpty(mutexName))
        {
            bool createdNew;
            using (var mutex = new Mutex(true, mutexName, out createdNew))
            {
                if (!createdNew && !string.IsNullOrEmpty(eventName))
                {
                    try
                    {
                        using (var handoff = EventWaitHandle.OpenExisting(eventName))
                        {
                            handoff.Set();
                        }
                    }
                    catch { }

                    if (!mutex.WaitOne(30000))
                    {
                        return 11;
                    }
                }
                Application.Run(form);
            }
        }
        else
        {
            Application.Run(form);
        }
        return 0;
    }
}
