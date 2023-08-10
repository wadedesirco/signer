# Linear Reward-sender

The Linear Reward-sender is a long-running process designed to automate the signing and publishing of weekly reward data for Linear Finance. It replaces the previously used combination of [Linear-finance/linear-reward-sender](https://github.com/Linear-finance/linear-reward-sender) and [Linear-finance/linear-reward-cli](https://github.com/Linear-finance/linear-reward-cli), providing a more maintainable solution.

## Prerequisites

Before running the Linear Reward-sender, ensure that you have Rust installed on your machine. If you haven't already, you can install Rust by following the instructions [here](https://www.rust-lang.org/tools/install).

Additionally, you will need to install `cargo-watch` by running the following command:

```bash
cargo install cargo-watch
```

## Development and Test on Local

To run the Linear Reward-sender on your local machine, follow these steps:

1. Create a new `.env` file from the provided `.env.sample` file. The `.env` file is used to set up the required environment variables for the reward-sender. You can modify the values in the `.env` file according to your specific configuration needs.

2. Build and run the reward-sender by running the following commands:

   ```bash
   cargo build
   ```

   ```bash
   cargo watch -x "run"
   ```

   These commands will build the necessary files and start the reward-sender in development mode. The reward-sender will be accessible on your local machine.

   **Note:** Ensure that you have the necessary permissions and access rights to run the reward-sender on your local machine.

## Deploy Docker Package

To deploy the Linear Reward-sender as a Docker package, follow these steps:

1. Create a new tag for the Docker image. For example:

   ```bash
   v0.0.1
   ```

   This tag helps identify the version of the Docker image.

2. Deploy the Docker image to your private package repository.

## Publish ECS in AWS

To publish the Linear Reward-sender on Amazon ECS (Elastic Container Service), follow these steps:

1. Create an ECS Cluster with at least one Container Instance. Refer to the [documentation](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/create-cluster-console-v2.html) for detailed instructions.

2. Create a Task Definition for the Linear Reward-sender. Refer to the [documentation](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/create-task-definition.html) for detailed instructions. When creating the task, make sure to add the necessary environment variables from the `.env.sample` file.

3. Create a Service that runs the Task Definition. Refer to the [documentation](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/v2-service-actions.html) for detailed instructions.

4. Confirm that everything is working as expected by testing the deployed Linear Reward-sender.

## License

The Linear Reward-sender is licensed under the [MIT License](https://choosealicense.com/licenses/mit/). This means that you have the freedom to use, modify, and distribute the reward-sender for both personal and commercial purposes. However, there is no warranty provided, and the developers are not liable for any damages or consequences arising from the use of the reward-sender.

Please refer to the license file for more details about the permissions and restrictions associated with the MIT License.