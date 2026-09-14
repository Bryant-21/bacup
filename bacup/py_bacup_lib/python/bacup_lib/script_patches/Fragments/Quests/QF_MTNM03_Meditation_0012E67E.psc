Function Fragment_Stage_0255_Item_00()
    If MTNM03_GuidedMeditationEndScene && !MTNM03_GuidedMeditationEndScene.IsPlaying()
        MTNM03_GuidedMeditationEndScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    Stop()
EndFunction
