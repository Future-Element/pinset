# Complete Temurin OpenJDK

Temurin 24 and newer enable [JEP 493](https://adoptium.net/news/2025/09/eclipse-temurin-25-available). Their official full JDK may omit the `jmods` directory: `jlink` uses the linkable runtime image instead. Pinset preserves that official layout and verifies runtime linking in acceptance; it does not fabricate packaged modules or download a second distribution. Cross-platform JMOD linking remains subject to upstream artifact availability.

Java is an independent Provider. Pinset installs the official Temurin JDK archive in full, with no JRE selection, vendor menu or payload trimming. Version selectors include a major version, `lts`, `latest` and an exact build such as `21.0.12.1+1`.

```sh
pinset use java@21
pinset which javac --explain
pinset exec -- javac Main.java
pinset exec -- java Main
pinset exec -- ./mvnw verify
pinset exec -- ./gradlew build
```

The lock fixes version, build, platform, official archive and checksum. The receipt inventories public tools from that exact archive. Java 8 and modern releases have different layouts and capabilities: missing `jshell`, `jlink`, `jfr` or `jpackage` is an explicit capability error, never a fallback to a system JDK.

The full archive retains compilation/runtime tools, jar/jarsigner/javadoc/javap/keytool, supported modules and jmods, jdeps/jmod/jlink/jpackage, jdb/jcmd/JFR/jconsole, JNI headers, standard-library sources, configuration, certificates, native libraries and licenses. Source-file launch and module tools apply only to releases that support them.

`JAVA_HOME`, shims and child processes share the precise SDK root. `which java --explain` includes the root, exact build, distribution, artifact and project/global binding source. Java 8's runtime `java.home` can point at the JDK's `jre` directory while `JAVA_HOME` remains the complete JDK root.

Maven, Gradle, JavaFX, frameworks and application packages remain project-managed. Wrappers can explicitly choose another compiler toolchain or reuse a daemon; their success alone is not evidence that every build stage used the locked JDK. Reports separate build runtime JDK, compiler toolchain declarations and bytecode targets. IDE project JDK and Java language-server runtime JDK are separate. The extension updates project runtimes without forcing an old project JDK onto the language server.

Local Docker acceptance covers Java 8/11/17/21/25/latest GA using fixed exact builds, plain applications, executable JARs, docs, source targets, modules/runtime images, JShell, debugger interaction, real JVM diagnostics/JFR, signing, JNI and wrapper builds. GUI reports distinguish installation inventory, startup and exercised interactions. Flutter/Android metadata and evidence contracts run separately. Actual Flutter SDK execution and APK builds are exempt from download tests by user request and remain unverified. Production behavior does not hard-code a Java major or mutate global Flutter JDK settings.
