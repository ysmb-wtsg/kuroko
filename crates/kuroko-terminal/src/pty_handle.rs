//! PTYプロセスの生成・管理を担当するモジュール。
//! portable-ptyを使ってシェルを起動し、読み書きのハンドルを提供する。

use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;

use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};

use kuroko_core::{KurokoError, PaneId};

/// PTYプロセスを管理し、読み取りデータをチャネル経由で配信する構造体。
pub struct PtyHandle {
    /// PTYマスター側の書き込みハンドル
    writer: Box<dyn Write + Send>,
    /// PTYマスターへの参照（リサイズ用）
    master: Box<dyn MasterPty + Send>,
}

/// PTY読み取りスレッドからメインスレッドに送信されるメッセージ
pub enum PtyMessage {
    /// PTYから読み取ったデータ
    Output { pane_id: PaneId, data: Vec<u8> },
    /// PTYプロセスが終了した
    Exited { pane_id: PaneId },
}

impl PtyHandle {
    /// 新しいPTYプロセスを生成し、読み取りスレッドを起動する。
    /// 読み取ったデータはmpscチャネル経由で送信される。
    ///
    /// @param pane_id - このPTYに紐づくペインID
    /// @param cols - ターミナルの列数
    /// @param rows - ターミナルの行数
    /// @param sender - 読み取りデータの送信先チャネル
    /// @returns PtyHandleインスタンス
    pub fn spawn(
        pane_id: PaneId,
        cols: u16,
        rows: u16,
        sender: mpsc::Sender<PtyMessage>,
    ) -> Result<Self, KurokoError> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(std::env::current_dir().unwrap_or_else(|_| "/".into()));
        Self::spawn_with_command(pane_id, cols, rows, sender, cmd)
    }

    /// PTYにバイト列を書き込む（キー入力の転送）。
    ///
    /// @param data - 書き込むバイト列
    pub fn write(&mut self, data: &[u8]) -> Result<(), KurokoError> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
    }

    /// 指定コマンドでPTYプロセスを生成する（エージェント用）。
    ///
    /// @param pane_id - このPTYに紐づくペインID
    /// @param cols - ターミナルの列数
    /// @param rows - ターミナルの行数
    /// @param sender - 読み取りデータの送信先チャネル
    /// @param cmd - 起動するコマンド
    /// @returns PtyHandleインスタンス
    pub fn spawn_with_command(
        pane_id: PaneId,
        cols: u16,
        rows: u16,
        sender: mpsc::Sender<PtyMessage>,
        cmd: CommandBuilder,
    ) -> Result<Self, KurokoError> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| KurokoError::Pty(e.to_string()))?;

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| KurokoError::Pty(e.to_string()))?;
        drop(pair.slave);

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| KurokoError::Pty(e.to_string()))?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| KurokoError::Pty(e.to_string()))?;

        // 読み取りスレッドで出力を転送し、終了後に子プロセスをwaitしてゾンビ化を防ぐ
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = sender.send(PtyMessage::Exited { pane_id });
                        break;
                    }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if sender.send(PtyMessage::Output { pane_id, data }).is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = child.wait();
        });

        Ok(Self {
            writer,
            master: pair.master,
        })
    }

    /// PTYのサイズを変更する。
    ///
    /// @param cols - 新しい列数
    /// @param rows - 新しい行数
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), KurokoError> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| KurokoError::Pty(e.to_string()))?;
        Ok(())
    }
}
