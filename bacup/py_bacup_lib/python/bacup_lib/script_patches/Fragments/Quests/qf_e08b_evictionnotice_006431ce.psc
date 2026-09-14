Function Fragment_Stage_0100_Item_00()
    If PA_EventStartRecording && !PA_EventStartRecording.IsPlaying()
        PA_EventStartRecording.Start()
    EndIf
EndFunction
