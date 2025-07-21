Shader "Unlit/FormantScroll"
{
    Properties
    {
        _PixelX("1/Width", Float) = 0.001953125
        _F1_L("F1_L", Float) = 0
        _F1_H("F1_H", Float) = 0
        _F2_L("F2_L", Float) = 0
        _F2_H("F2_H", Float) = 0
        _F3_L("F3_L", Float) = 0
        _F3_H("F3_H", Float) = 0
        _F4_L("F4_L", Float) = 0
        _F4_H("F4_H", Float) = 0
        _G1("G1", Float) = 0
    }

        SubShader
    {
        Tags { "RenderType" = "Opaque" }
        GrabPass { "_GrabPassTex" }

        Pass
        {
            CGPROGRAM
            #pragma vertex vert
            #pragma fragment frag
            #include "UnityCG.cginc"

            float _PixelX;
            float _F1_L, _F1_H, _F2_L, _F2_H, _F3_L, _F3_H, _F4_L, _F4_H;
            float _G1;

            sampler2D _GrabPassTex;

            struct v2f
            {
                float2 uv : TEXCOORD0;
                float4 vertex : SV_POSITION;
            };

            v2f vert(float4 vertex : POSITION)
            {
                v2f o;
                o.vertex = UnityObjectToClipPos(vertex);
                o.uv = ComputeGrabScreenPos(o.vertex);
                return o;
            }

            float decodeFreq(float low, float high)
            {
                float f = 0.0;
                if (low > 0.0) f += 1.0 / low;
                if (high > 0.0) f += (1.0 / high) * 128.0;
                return f;
            }

            half4 frag(v2f i) : SV_Target
            {
                float MAX_HZ = 4096.0;
                float y = i.uv.y;
                float x = i.uv.x;

                // 前のフレームを左にスクロール
                float2 src = i.uv + float2(_PixelX, 0.0);
                half4 ret = (src.x <= 1.0)
                    ? tex2D(_GrabPassTex, src)
                    : half4(0, 0, 0, 1); // 右端は毎フレーム新しく描画

                // === 罫線描画 ===
                float lineWidth = _PixelX * 1.0;
                float lineIntensity = 0.0;

                // 1kHzごとの太線
                [unroll]
                for (int k = 1; k <= 4; ++k)
                {
                    float freq = 1000.0 * k;
                    float yLine = freq / MAX_HZ;
                    float dist = abs(i.uv.y - yLine);
                    lineIntensity = max(lineIntensity, smoothstep(lineWidth, 0.0, dist));
                }

                // 500Hzごとの薄線（1kHz除外）
                [unroll]
                for (int k = 1; k <= 7; ++k)
                {
                    float freq = 500.0 * k;
                    if (freq % 1000 == 0) continue; // 重複回避
                    float yLine = freq / MAX_HZ;
                    float dist = abs(i.uv.y - yLine);
                    lineIntensity = max(lineIntensity, smoothstep(lineWidth, 0.0, dist) * 0.3);
                }

                float3 gridColor = float3(1.0, 1.0, 1.0) * lineIntensity;
                ret.rgb = max(ret.rgb, gridColor);

                // 右端にフォルマントをプロット
                if (x > 1.0 - _PixelX * 1.5 && _G1 > 0.01)
                {
                    float f1 = decodeFreq(_F1_L, _F1_H);
                    float f2 = decodeFreq(_F2_L, _F2_H);
                    float f3 = decodeFreq(_F3_L, _F3_H);
                    float f4 = decodeFreq(_F4_L, _F4_H);

                    float f1y = f1 / MAX_HZ;
                    float f2y = f2 / MAX_HZ;
                    float f3y = f3 / MAX_HZ;
                    float f4y = f4 / MAX_HZ;

                    float3 colorF1 = float3(1, 0, 0);
                    float3 colorF2 = float3(0, 1, 0);
                    float3 colorF3 = float3(0, 0, 1);
                    float3 colorF4 = float3(1, 0, 1);

                    float band = _PixelX * 2.0; // 点の縦幅

                    float3 col = float3(0, 0, 0);
                    col += colorF1 * smoothstep(band, 0.0, abs(y - f1y));
                    col += colorF2 * smoothstep(band, 0.0, abs(y - f2y));
                    col += colorF3 * smoothstep(band, 0.0, abs(y - f3y));
                    col += colorF4 * smoothstep(band, 0.0, abs(y - f4y));

                    ret.rgb = max(ret.rgb, col); // 上書きではなく加算
                }

                return ret;
            }
            ENDCG
        }
    }
}
