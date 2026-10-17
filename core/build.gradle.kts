plugins { kotlin("jvm") }
kotlin { jvmToolchain(17) }
dependencies {
    implementation("com.google.code.gson:gson:2.13.1")
    testImplementation(kotlin("test"))
    testImplementation("org.junit.jupiter:junit-jupiter:5.12.2")
    testImplementation("org.xerial:sqlite-jdbc:3.49.1.0")
}
tasks.test { useJUnitPlatform(); testLogging { events("passed", "skipped", "failed") } }
