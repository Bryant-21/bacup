; Objective 50 (read the label) and the riddle objectives 60-66 depend on two independent events: reading the label (440) and blacking out (450), in either order.
Function ShowRiddleObjectives()
    SetObjectiveDisplayed(60, True)
    SetObjectiveDisplayed(61, True)
    SetObjectiveDisplayed(62, True)
    SetObjectiveDisplayed(63, True)
    SetObjectiveDisplayed(64, True)
    SetObjectiveDisplayed(65, True)
    SetObjectiveDisplayed(66, True)
EndFunction

Function StartPartyWave(String asWaveID)
    DefaultQuestEncounterWaveScript waves = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waves != None
        waves.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(5, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(35, True)
    SetObjectiveDisplayed(40, True)
    StartPartyWave("Party Ghouls Wave 1")
EndFunction

Function Fragment_Stage_0410_Item_00()
    StartPartyWave("Party Ghouls Wave 2")
EndFunction

Function Fragment_Stage_0420_Item_00()
    SetObjectiveCompleted(40, True)
EndFunction

Function Fragment_Stage_0440_Item_00()
    If IsStageDone(450)
        SetObjectiveCompleted(50, True)
        ShowRiddleObjectives()
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveCompleted(35, True)
    If IsStageDone(440)
        ShowRiddleObjectives()
    Else
        SetObjectiveDisplayed(50, True)
    EndIf
EndFunction

Function Fragment_Stage_0575_Item_00()
    SetObjectiveCompleted(60, True)
    SetObjectiveCompleted(61, True)
    SetObjectiveCompleted(62, True)
    SetObjectiveCompleted(63, True)
    SetObjectiveCompleted(64, True)
    SetObjectiveCompleted(65, True)
    SetObjectiveCompleted(66, True)
    SetObjectiveDisplayed(68, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(68, True)
    SetObjectiveDisplayed(70, True)
EndFunction

Function Fragment_Stage_0650_Item_00()
    SetObjectiveCompleted(70, True)
    SetObjectiveDisplayed(75, True)
EndFunction

Function Fragment_Stage_0660_Item_00()
    SetObjectiveDisplayed(77, True)
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0690_Item_00()
    SetObjectiveCompleted(77, True)
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(78, True)
EndFunction

Function Fragment_Stage_0695_Item_00()
    SetObjectiveCompleted(78, True)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(75, True)
    If !IsStageDone(750)
        SetStage(750)
    EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
    ; TODO(recipes): FO76 teaches co_Brewing_NukaShine, workshop_co_Crafting_Brewing_Bench, workshop_co_Crafting_Brewing_Fermenter at this stage (Server76 2026-10-09). Tales leaves quest-only recipes ungated today; teach them here once recipes are learn-gated.
    SetObjectiveDisplayed(80, True)
    SetObjectiveDisplayed(81, True)
    SetObjectiveDisplayed(82, True)
    SetObjectiveDisplayed(83, True)
    SetObjectiveDisplayed(84, True)
    SetObjectiveDisplayed(85, True)
    SetObjectiveDisplayed(86, True)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(80, True)
    SetObjectiveCompleted(81, True)
    SetObjectiveCompleted(82, True)
    SetObjectiveCompleted(83, True)
    SetObjectiveCompleted(84, True)
    SetObjectiveCompleted(85, True)
    SetObjectiveCompleted(86, True)
    SetObjectiveCompleted(88, True)
    SetObjectiveDisplayed(90, True)
    SetObjectiveDisplayed(95, True)
EndFunction

Function Fragment_Stage_0850_Item_00()
    SetObjectiveCompleted(95, True)
    SetObjectiveDisplayed(96, True)
EndFunction

Function Fragment_Stage_0875_Item_00()
    SetObjectiveCompleted(96, True)
    SetObjectiveCompleted(90, True)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(90, True)
    SetObjectiveCompleted(95, True)
    If IsObjectiveDisplayed(96)
        SetObjectiveCompleted(96, True)
    EndIf
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(100, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction
