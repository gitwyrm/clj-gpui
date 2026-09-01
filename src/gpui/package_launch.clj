(ns gpui.package-launch
  "POSIX launcher script for packaged clj-gpui apps." )

(defn launcher-script
  "POSIX launcher that starts the bundled JVM, which then starts the host."
  [{:keys [name main]}]
  (str "#!/bin/sh\n"
       "set -eu\n"
       "here=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\n"
       "app_home=${CLJ_GPUI_APP_HOME:-$here}\n"
       "# Walk up from MacOS/ or bin/ to the runtime/jar layout.\n"
       "if [ -x \"$here/clj-gpui-host\" ]; then host=\"$here/clj-gpui-host\"\n"
       "elif [ -x \"$app_home/clj-gpui-host\" ]; then host=\"$app_home/clj-gpui-host\"\n"
       "elif [ -x \"$app_home/bin/clj-gpui-host\" ]; then host=\"$app_home/bin/clj-gpui-host\"\n"
       "else echo \"$0: bundled clj-gpui host not found\" >&2; exit 1; fi\n"
       "if [ -x \"$here/../runtime/bin/java\" ]; then java_home=\"$here/../runtime\"\n"
       "elif [ -x \"$here/../Resources/runtime/bin/java\" ]; then java_home=\"$here/../Resources/runtime\"\n"
       "elif [ -x \"$app_home/runtime/bin/java\" ]; then java_home=\"$app_home/runtime\"\n"
       "else echo \"$0: bundled Java runtime not found\" >&2; exit 1; fi\n"
       "if [ -f \"$here/../Resources/" name ".jar\" ]; then jar=\"$here/../Resources/" name ".jar\"\n"
       "elif [ -f \"$here/../lib/" name ".jar\" ]; then jar=\"$here/../lib/" name ".jar\"\n"
       "elif [ -f \"$app_home/lib/" name ".jar\" ]; then jar=\"$app_home/lib/" name ".jar\"\n"
       "elif [ -f \"$here/" name ".jar\" ]; then jar=\"$here/" name ".jar\"\n"
       "else echo \"$0: bundled application jar not found\" >&2; exit 1; fi\n"
       "host_dir=$(CDPATH= cd -- \"$(dirname -- \"$host\")\" && pwd)\n"
       "export CLJ_GPUI_BIN=\"$host\"\n"
       "export CLJ_GPUI_APP_HOME=\"$host_dir\"\n"
       "export JAVA_HOME=\"$java_home\"\n"
       "exec \"$java_home/bin/java\" -Djava.awt.headless=true -cp \"$jar\" gpui.prod " main "\n"))
