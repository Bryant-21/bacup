; FO76 set up the server-side horde here; GQ_HordeQuestScript.B21BeginRoaming does it in the conversion.
Function Fragment_Stage_0001_Item_00()
EndFunction

; Item 0 always runs; item 1 is conditioned on the dungeon search, which the conversion skips.
Function Fragment_Stage_0100_Item_00()
    GQ_HordeQuestScript horde = (Self as Quest) as GQ_HordeQuestScript
    If horde != None && horde.SkippedDungeon
        SetObjectiveDisplayed(200, True)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_01()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    If IsObjectiveDisplayed(100)
        SetObjectiveCompleted(100, True)
    EndIf
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    CompleteAllObjectives()
EndFunction
