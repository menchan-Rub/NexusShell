fn main() -> Result<(), Box<dyn std::error::Error>> {
    // protobuf生成を一時的に無効化（protocが必要）
    // tonic_build::configure()
    //     .build_server(true)
    //     .build_client(false)
    //     .out_dir("src/generated")
    //     .compile(&["proto/nexus.proto"], &["proto/"])?;
    
    println!("cargo:rerun-if-changed=proto/nexus.proto");
    Ok(())
} 