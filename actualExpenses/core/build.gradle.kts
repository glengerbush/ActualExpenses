plugins { kotlin("jvm") }
kotlin { jvmToolchain(17) }
dependencies {
    implementation("com.google.code.gson:gson:2.13.1")
    implementation("com.squareup.okhttp3:okhttp:5.3.0")
    testImplementation(kotlin("test"))
    testImplementation("org.junit.jupiter:junit-jupiter:5.12.2")
    testImplementation("org.xerial:sqlite-jdbc:3.49.1.0")
}
tasks.test {
    useJUnitPlatform()
    // Fixture authoring is opt-in; ordinary acceptance must execute every selected test.
    if (!providers.environmentVariable("ACTUAL_UPGRADE_FIXTURE_OUTPUT").isPresent) {
        exclude("**/LegacyUpgradeFixtureGeneratorTest*")
    }
    testLogging { events("passed", "skipped", "failed") }
}
