; TODO

Function Fragment_Stage_2100_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_PlayerKnows_BeenToVault79, 1.0)
    EndIf
    If IsStageDone(2200) && !IsStageDone(2300)
        SetStage(2300)
    EndIf
EndFunction

Function Fragment_Stage_2200_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pW05_MQ00_CodeAV, 1.0)
    EndIf
    If IsStageDone(2100) && !IsStageDone(2300)
        SetStage(2300)
    EndIf
EndFunction

Function Fragment_Stage_2300_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pW05_MQ00_Completed, 1.0)
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction
