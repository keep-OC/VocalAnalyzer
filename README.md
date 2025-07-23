# VocalAnalyzer

## 概要

VRChat 上でグローバルに動作する音声解析ツールです。主な機能は下記の 3 点です。

- サウンドスペクトログラム (Spectrogram)
  - 基音とその整数次倍音のゲインをヒートマップで表示します
  - 0 Hz から 8192 Hz を表示します
  - デフォルトでは 10 倍音まで表示します
  - カスタムで 20 倍音まで表示を拡張可能です（詳細は後述します）
  - デフォルトで expression parameter を 83 bits 消費します（20 倍音まで表示する場合は 163 bits 消費します）
- ピッチモニター (PitchMonitor)
  - E2 から G5 を表示します
  - expression parameter を 19 bits 消費します
- フォルマントモニター (FormantMonitor)
  - 第一フォルマントから第四フォルマントを表示します
  - expression parameter を 67 bits 消費します

## 開発環境

- Unity version 2022.3.22f1
- VRChat SDK 3.7.6
- Modular Avatar 1.13.0

## 導入方法

VocalAnalyzer を使用するためには **アセットが組み込まれたアバターのアップロード** と、**OSC アプリの実行** が必要です。

### アセットが組み込まれたアバターのアップロード

VocalAnalyzer.unitypackage をアバターのプロジェクトにインポートし、使用した機能に対応した prefab をアバター直下に配置してください。  
Modular Avalar に対応しているため、prefab をアバター直下に配置してアップロードするだけで動作するはずです。

### OSC アプリの実行

1. VocalAnalyzer を使用したいタイミングで VocalAnalyzer.exe を実行してください
2. 音声入力に使いたいマイクを選択してください
3. start ボタンを押してから、音声を入力し、インジケーターが 6~7 割の位置にくるようにスライダーの位置を調整してください
4. start が押された後、stop ボタンが押されるまでは常に音声解析結果が OSC で送信されています

### 注意事項

- 全ての機能をまるっと含めると expression parameter が不足する可能性があるのでお気をつけください

### その他

- スペクトログラムの表示倍音を増やす（減らす）方法
  - Spectrogram の MA parameters から *G~~* (ex. G10, G11)のパラメータを増やしたり減らしたりしてください
  - *G~~* のパラメータを追加した際に名前の右にあるタブが "Animatorのみ" になる場合、"Float" に変更してください
- アップロードすると表示用の Quad が変な位置（頭のはるか上）などにある場合
  - Spectrogram(or PitchMonitor, FormantMonitor)/Other/BoneProxy のターゲットを Head ではなく例えば hand R などに変更し、Spectrogram/Other/Constraint の Zero を設定してください
    - 右手に表示用 Quad が追従してくるはずなので、使うときは適当な位置で FixPosition してください
