use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose, SanType,
};
use tempfile::TempDir;

pub struct TlsMaterial {
    _temp: TempDir,
    pub ca_cert_pem: Vec<u8>,
    pub ca_cert_path: PathBuf,
    pub leaf_cert_path: PathBuf,
    pub leaf_key_path: PathBuf,
    pub minio_certs_dir: PathBuf,
    pub rustfs_tls_dir: PathBuf,
}

impl TlsMaterial {
    pub fn generate() -> Self {
        let temp = TempDir::new().expect("create tls tempdir");

        let mut ca_params = CertificateParams::default();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::CrlSign,
        ];
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "abixio bench CA");
        let ca_key = KeyPair::generate().expect("generate CA key");
        let ca_cert = ca_params.self_signed(&ca_key).expect("generate CA cert");

        let mut leaf_params =
            CertificateParams::new(vec!["localhost".to_string()]).expect("leaf params");
        leaf_params
            .subject_alt_names
            .push(SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        leaf_params
            .distinguished_name
            .push(DnType::CommonName, "localhost");
        leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        leaf_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        let leaf_key = KeyPair::generate().expect("generate leaf key");
        let leaf_cert = leaf_params
            .signed_by(&leaf_key, &ca_cert, &ca_key)
            .expect("sign leaf cert");

        let ca_cert_pem = ca_cert.pem().into_bytes();
        let leaf_cert_pem = leaf_cert.pem().into_bytes();
        let leaf_key_pem = leaf_key.serialize_pem().into_bytes();

        let ca_cert_path = temp.path().join("ca.crt");
        let leaf_cert_path = temp.path().join("tls-cert.pem");
        let leaf_key_path = temp.path().join("tls-key.pem");
        std::fs::write(&ca_cert_path, &ca_cert_pem).expect("write ca cert");
        std::fs::write(&leaf_cert_path, &leaf_cert_pem).expect("write leaf cert");
        std::fs::write(&leaf_key_path, &leaf_key_pem).expect("write leaf key");

        let minio_certs_dir = temp.path().join("minio-certs");
        std::fs::create_dir_all(minio_certs_dir.join("CAs")).expect("create minio cert dirs");
        std::fs::write(minio_certs_dir.join("public.crt"), &leaf_cert_pem)
            .expect("write minio cert");
        std::fs::write(minio_certs_dir.join("private.key"), &leaf_key_pem)
            .expect("write minio key");
        std::fs::write(minio_certs_dir.join("CAs").join("public.crt"), &ca_cert_pem)
            .expect("write minio ca");

        let rustfs_tls_dir = temp.path().join("rustfs-tls");
        std::fs::create_dir_all(&rustfs_tls_dir).expect("create rustfs tls dir");
        std::fs::write(rustfs_tls_dir.join("rustfs_cert.pem"), &leaf_cert_pem)
            .expect("write rustfs cert");
        std::fs::write(rustfs_tls_dir.join("rustfs_key.pem"), &leaf_key_pem)
            .expect("write rustfs key");
        std::fs::write(rustfs_tls_dir.join("ca.crt"), &ca_cert_pem).expect("write rustfs ca");
        std::fs::write(rustfs_tls_dir.join("public.crt"), &ca_cert_pem)
            .expect("write rustfs public ca");

        Self {
            _temp: temp,
            ca_cert_pem,
            ca_cert_path,
            leaf_cert_path,
            leaf_key_path,
            minio_certs_dir,
            rustfs_tls_dir,
        }
    }
}
