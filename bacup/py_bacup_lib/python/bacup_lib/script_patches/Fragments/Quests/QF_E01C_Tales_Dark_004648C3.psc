Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
    If Scene_01 && !Scene_01.IsPlaying()
        Scene_01.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(25)
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(25)
    If Scene_02 && !Scene_02.IsPlaying()
        Scene_02.Start()
    EndIf
EndFunction

Function Fragment_Stage_0380_Item_00()
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30)
    If Scene_03 && !Scene_03.IsPlaying()
        Scene_03.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(40)
    If Scene_04 && !Scene_04.IsPlaying()
        Scene_04.Start()
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0850_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
    If Scene_BringHolo && !Scene_BringHolo.IsPlaying()
        Scene_BringHolo.Start()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(60)
    ; Only the Ronnie / Bug Swarm ending (stage 120) runs the numbered insect waves
    ; that objective 85 and these three messages describe.
    If GetStageDone(120) && E01C_Tales_Dark_Wave1
        E01C_Tales_Dark_Wave1.Show()
    EndIf
EndFunction

Function Fragment_Stage_0921_Item_00()
    If E01C_Tales_Dark_Wave2
        E01C_Tales_Dark_Wave2.Show()
    EndIf
EndFunction

Function Fragment_Stage_0922_Item_00()
    If E01C_Tales_Dark_WaveFinal
        E01C_Tales_Dark_WaveFinal.Show()
    EndIf
EndFunction

Function Fragment_Stage_9991_Item_00()
    SetObjectiveFailed(10)
EndFunction
