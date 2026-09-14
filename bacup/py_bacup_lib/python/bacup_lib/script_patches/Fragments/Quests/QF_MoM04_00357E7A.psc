Function Fragment_Stage_0020_Item_00()
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(40)
    SetObjectiveDisplayed(41)
EndFunction

Function Fragment_Stage_0041_Item_00()
    If IsStageDone(42) && IsStageDone(43) && !IsStageDone(45)
        SetStage(45)
    EndIf
EndFunction

Function Fragment_Stage_0042_Item_00()
    If IsStageDone(41) && IsStageDone(43) && !IsStageDone(45)
        SetStage(45)
    EndIf
EndFunction

Function Fragment_Stage_0043_Item_00()
    If IsStageDone(41) && IsStageDone(42) && !IsStageDone(45)
        SetStage(45)
    EndIf
EndFunction

Function Fragment_Stage_0045_Item_00()
    SetObjectiveCompleted(41)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0051_Item_00()
    If IsStageDone(52) && !IsStageDone(60)
        SetStage(60)
    EndIf
EndFunction

Function Fragment_Stage_0052_Item_00()
    If IsStageDone(51) && !IsStageDone(60)
        SetStage(60)
    EndIf
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0090_Item_00()
    SetObjectiveCompleted(60)
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor player = Alias_ActivePlayer.GetActorReference()
    ObjectReference mistressHolotape = Alias_MoM04MistressHolotape.GetReference()
    If player && mistressHolotape && mistressHolotape.GetContainer() != player
        player.AddItem(mistressHolotape, 1, True)
    EndIf
    If player && MoMRank
        player.SetValue(MoMRank, 4.0)
    EndIf

    CompleteAllObjectives()
    Stop()
EndFunction
