Function Fragment_Stage_0001_Item_00()
    SetObjectiveDisplayed(410, False)
    SetObjectiveDisplayed(420, False)
EndFunction

Function Fragment_Stage_0005_Item_00()
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0006_Item_00()
    ObjectReference mapDispenser = Alias_Container_Map.GetReference()
    If mapDispenser != None
        mapDispenser.Enable(False)
    EndIf
    ObjectReference introDispenser = Alias_Dispenser_Holotape_Intro.GetReference()
    If introDispenser != None
        introDispenser.Enable(False)
    EndIf
    ObjectReference catalogDispenser = Alias_Dispenser_Book_Catalog.GetReference()
    If catalogDispenser != None
        catalogDispenser.Enable(False)
    EndIf
    ObjectReference letterDispenser = Alias_Dispenser_Book_Letter.GetReference()
    If letterDispenser != None
        letterDispenser.Enable(False)
    EndIf
EndFunction

Function Fragment_Stage_0007_Item_00()
    ObjectReference janelleDispenser = Alias_Dispenser_Holotape_Janelle.GetReference()
    If janelleDispenser != None
        janelleDispenser.Enable(False)
    EndIf
    ObjectReference raymondDispenser = Alias_Dispenser_Holotape_Raymond.GetReference()
    If raymondDispenser != None
        raymondDispenser.Enable(False)
    EndIf
    ObjectReference noteDispenser = Alias_Dispenser_Book_Note.GetReference()
    If noteDispenser != None
        noteDispenser.Enable(False)
    EndIf
    ObjectReference directionsDispenser = Alias_Dispenser_Book_Directions.GetReference()
    If directionsDispenser != None
        directionsDispenser.Enable(False)
    EndIf
EndFunction

Function Fragment_Stage_0008_Item_00()
    ObjectReference dispenserRef = Alias_Dispenser_Holotape_Intro.GetReference()
    If dispenserRef != None
        dispenserRef.Disable(False)
    EndIf
    dispenserRef = Alias_Dispenser_Holotape_Janelle.GetReference()
    If dispenserRef != None
        dispenserRef.Disable(False)
    EndIf
    dispenserRef = Alias_Dispenser_Holotape_Raymond.GetReference()
    If dispenserRef != None
        dispenserRef.Disable(False)
    EndIf
    dispenserRef = Alias_Dispenser_Book_Note.GetReference()
    If dispenserRef != None
        dispenserRef.Disable(False)
    EndIf
    dispenserRef = Alias_Dispenser_Book_Letter.GetReference()
    If dispenserRef != None
        dispenserRef.Disable(False)
    EndIf
    dispenserRef = Alias_Dispenser_Book_Directions.GetReference()
    If dispenserRef != None
        dispenserRef.Disable(False)
    EndIf
    dispenserRef = Alias_Dispenser_Book_Catalog.GetReference()
    If dispenserRef != None
        dispenserRef.Disable(False)
    EndIf
EndFunction

Function Fragment_Stage_0011_Item_00()
    SetObjectiveCompleted(200, True)
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0012_Item_00()
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0013_Item_00()
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0014_Item_00()
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0015_Item_00()
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0021_Item_00()
    SetObjectiveCompleted(410, True)
EndFunction

Function Fragment_Stage_0022_Item_00()
    SetObjectiveCompleted(420, True)
EndFunction

Function Fragment_Stage_0024_Item_00()
    SetObjectiveDisplayed(450, True)
EndFunction

Function Fragment_Stage_0025_Item_00()
    SetObjectiveCompleted(450, True)
    SetObjectiveDisplayed(500, True)
    defaultquestencounterwavescript encounterWaves = (Self as Quest) as defaultquestencounterwavescript
    If encounterWaves != None
        encounterWaves.StartLocalEncounterWave(0)
    EndIf
EndFunction

Function Fragment_Stage_0026_Item_00()
    SetObjectiveDisplayed(500, True)
EndFunction

Function Fragment_Stage_0027_Item_00()
    SetObjectiveCompleted(500, True)
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_0028_Item_00()
    SetObjectiveDisplayed(450, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    ActorValue caseStatus = Game.GetFormFromFile(0x0047F445, "SeventySix.esm") as ActorValue
    If playerRef != None && caseStatus != None
        playerRef.SetValue(caseStatus, 1.0)
    EndIf
    SetObjectiveDisplayed(100, True)
    If !IsStageDone(6)
        SetStage(6)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300, True)
    SetObjectiveDisplayed(350, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(350, True)
    SetObjectiveDisplayed(400, True)
    SetObjectiveDisplayed(410, True)
    SetObjectiveDisplayed(420, True)
    SetObjectiveDisplayed(450, True)
    If !IsStageDone(7)
        SetStage(7)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    ActorValue caseStatus = Game.GetFormFromFile(0x0047F445, "SeventySix.esm") as ActorValue
    SetObjectiveCompleted(400, True)
    SetObjectiveCompleted(410, True)
    SetObjectiveCompleted(420, True)
    SetObjectiveCompleted(450, True)
    SetObjectiveCompleted(500, True)
    If playerRef != None && caseStatus != None
        playerRef.SetValue(caseStatus, 2.0)
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_7999_Item_00()
    If !IsStageDone(8)
        SetStage(8)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    ActorValue caseStatus = Game.GetFormFromFile(0x0047F445, "SeventySix.esm") as ActorValue
    Quest masterQuest = Game.GetFormFromFile(0x0047F444, "SeventySix.esm") as Quest
    SetObjectiveCompleted(100, True)
    SetObjectiveCompleted(200, True)
    SetObjectiveCompleted(300, True)
    SetObjectiveCompleted(350, True)
    SetObjectiveCompleted(400, True)
    SetObjectiveCompleted(410, True)
    SetObjectiveCompleted(420, True)
    SetObjectiveCompleted(450, True)
    SetObjectiveCompleted(500, True)
    If !IsStageDone(8)
        SetStage(8)
    EndIf
    CompleteQuest()
    If playerRef != None && caseStatus != None
        playerRef.SetValue(caseStatus, 3.0)
    EndIf
    If masterQuest != None
        If IsStageDone(11) && IsStageDone(12) && IsStageDone(13) && IsStageDone(14) && IsStageDone(15) && IsStageDone(21) && IsStageDone(22) && IsStageDone(23) && IsStageDone(24) && IsStageDone(26) && IsStageDone(28)
            masterQuest.SetStage(5010)
        Else
            masterQuest.SetStage(5000)
        EndIf
    EndIf
EndFunction
