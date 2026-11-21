#[test]
fn recorded_cases() {
    trycmd::TestCases::new()
        .register_bin(
            "run_tracker",
            std::path::Path::new(env!("CARGO_BIN_EXE_run_tracker")),
        )
        .case("app-cases/*.toml");
}
