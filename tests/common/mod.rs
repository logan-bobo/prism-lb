use std::sync::Arc;

use tokio::{fs::File, io::AsyncReadExt};

use bollard::{
    secret::{ContainerCreateBody, HostConfig, PortBinding},
    Docker,
};
use futures_util::stream::TryStreamExt;
use std::collections::HashMap;

use prism_lb::{config::Config, server::Server};

const NGINX_IMAGE: &str = "nginx:latest";

pub struct TestContainers {
    docker: Docker,
    container_names: Vec<String>,
}

impl Drop for TestContainers {
    fn drop(&mut self) {
        let docker = self.docker.clone();
        let names = self.container_names.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                for name in &names {
                    docker
                        .remove_container(
                            &name,
                            Some(bollard::query_parameters::RemoveContainerOptionsBuilder::default()
                                .force(true)
                                .build()),
                        )
                        .await
                        .unwrap();
                }
            });
        }).join().unwrap();
    }
}

pub async fn run_test_containers() -> TestContainers {
    let docker = Docker::connect_with_socket_defaults().unwrap();

    // TODO: make all of this random! dynamic ports!
    // for now this is fine just to validate initial test cases
    let nginx_one = build_container_config("5001".to_string(), 0);
    let nginx_two = build_container_config("5002".to_string(), 1);
    let nginx_three = build_container_config("5003".to_string(), 2);

    let test_containers = vec![nginx_one, nginx_two, nginx_three];
    let mut running_container_names = Vec::<String>::new();

    let _ = &docker
        .create_image(
            Some(
                bollard::query_parameters::CreateImageOptionsBuilder::default()
                    .from_image(NGINX_IMAGE)
                    .build(),
            ),
            None,
            None,
        )
        .try_collect::<Vec<_>>()
        .await
        .unwrap();

    for (i, v) in test_containers.iter().enumerate() {
        let container_name = format!("nginx-{}", i);

        let _ = &docker
            .create_container(
                Some(
                    bollard::query_parameters::CreateContainerOptionsBuilder::default()
                        .name(&container_name)
                        .build(),
                ),
                v.clone(),
            )
            .await
            .unwrap();

        let _ = &docker
            .start_container(
                &container_name,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await
            .unwrap();

        running_container_names.push(container_name);
    }

    TestContainers {
        docker,
        container_names: running_container_names,
    }
}

fn build_container_config(host_port: String, instance_id: usize) -> ContainerCreateBody {
    let mut nginx_bindings = HashMap::<String, Option<Vec<PortBinding>>>::new();
    nginx_bindings.insert(
        String::from("80/tcp"),
        Some(vec![PortBinding {
            host_ip: Some(String::from("127.0.0.1")),
            host_port: Some(String::from(host_port)),
        }]),
    );

    ContainerCreateBody {
        image: Some(NGINX_IMAGE.to_string()),
        host_config: Some(HostConfig {
            port_bindings: Some(nginx_bindings),
            ..Default::default()
        }),
        cmd: Some(vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("echo '<p>instance-{}</p>' > /usr/share/nginx/html/index.html && nginx -g 'daemon off;'", instance_id)
        ]),
        ..Default::default()
    }
}

async fn build_test_server_config() -> Config {
    let mut file = File::open("./tests/fixtures/test.yaml").await.unwrap();

    let mut contents = String::new();

    file.read_to_string(&mut contents).await.unwrap();

    Config::try_from(contents).unwrap()
}

pub async fn test_server() -> Arc<Server> {
    let config = build_test_server_config().await;

    Arc::new(Server::new(config).await.unwrap())
}
