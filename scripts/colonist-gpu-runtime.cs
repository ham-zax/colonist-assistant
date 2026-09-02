using System;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;

internal static class Program
{
    private const string ConfigFileName = "gpu-runtime.conf";

    private sealed class RuntimeConfig
    {
        public string Distro;
        public string HostPath;
    }

    private static int Main()
    {
        try
        {
            var configPath = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, ConfigFileName);
            var config = LoadConfig(configPath);
            var wsl = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.System),
                "wsl.exe"
            );

            if (!File.Exists(wsl))
            {
                throw new FileNotFoundException("wsl.exe was not found", wsl);
            }

            var startInfo = new ProcessStartInfo
            {
                FileName = wsl,
                Arguments = "-d " + config.Distro + " --exec " + config.HostPath,
                UseShellExecute = false,
                RedirectStandardInput = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true,
            };

            using (var child = new Process { StartInfo = startInfo })
            {
                if (!child.Start())
                {
                    throw new InvalidOperationException("wsl.exe did not start");
                }

                StartPump(
                    Console.OpenStandardInput(),
                    child.StandardInput.BaseStream,
                    true
                );
                var errorThread = StartPump(
                    child.StandardError.BaseStream,
                    Console.OpenStandardError(),
                    false
                );

                CopyAndFlush(child.StandardOutput.BaseStream, Console.OpenStandardOutput());
                child.WaitForExit();
                errorThread.Join(1000);

                if (child.ExitCode != 0)
                {
                    Console.Error.WriteLine(
                        "[Colonist GPU Runtime] WSL companion exited with code {0} " +
                        "(distro={1}, host={2}).",
                        child.ExitCode,
                        config.Distro,
                        config.HostPath
                    );
                }

                return child.ExitCode;
            }
        }
        catch (Exception error)
        {
            Console.Error.WriteLine("[Colonist GPU Runtime] {0}", error.Message);
            return 1;
        }
    }

    private static Thread StartPump(Stream source, Stream destination, bool closeDestination)
    {
        var thread = new Thread(delegate()
        {
            try
            {
                CopyAndFlush(source, destination);
            }
            catch (IOException)
            {
                // The opposite side can close first during normal Native Messaging shutdown.
            }
            catch (ObjectDisposedException)
            {
                // The child can exit while the browser still owns the runtime input pipe.
            }
            finally
            {
                if (closeDestination)
                {
                    try
                    {
                        destination.Close();
                    }
                    catch (IOException)
                    {
                    }
                }
            }
        });
        thread.IsBackground = true;
        thread.Start();
        return thread;
    }

    private static void CopyAndFlush(Stream source, Stream destination)
    {
        var buffer = new byte[16 * 1024];
        int count;
        while ((count = source.Read(buffer, 0, buffer.Length)) > 0)
        {
            destination.Write(buffer, 0, count);
            destination.Flush();
        }
    }

    private static RuntimeConfig LoadConfig(string path)
    {
        if (!File.Exists(path))
        {
            throw new FileNotFoundException(
                "runtime configuration is missing; reinstall the Colonist GPU Runtime",
                path
            );
        }

        string distro = null;
        string hostPath = null;
        foreach (var rawLine in File.ReadAllLines(path, Encoding.UTF8))
        {
            var line = rawLine.Trim();
            if (line.Length == 0 || line.StartsWith("#", StringComparison.Ordinal))
            {
                continue;
            }

            var separator = line.IndexOf('=');
            if (separator <= 0)
            {
                throw new InvalidDataException("invalid runtime configuration line");
            }

            var key = line.Substring(0, separator).Trim();
            var value = line.Substring(separator + 1).Trim();
            if (key == "distro" && distro == null)
            {
                distro = value;
            }
            else if (key == "host" && hostPath == null)
            {
                hostPath = value;
            }
            else
            {
                throw new InvalidDataException("invalid or duplicate runtime configuration key: " + key);
            }
        }

        if (string.IsNullOrEmpty(distro) ||
            !Regex.IsMatch(distro, "^[A-Za-z0-9._-]+$"))
        {
            throw new InvalidDataException("configured WSL distribution is missing or invalid");
        }
        if (string.IsNullOrEmpty(hostPath) ||
            !Regex.IsMatch(hostPath, "^/[A-Za-z0-9._/+.-]+$"))
        {
            throw new InvalidDataException(
                "configured Linux companion path must be an absolute path without shell syntax"
            );
        }

        return new RuntimeConfig { Distro = distro, HostPath = hostPath };
    }
}
