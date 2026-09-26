CB02_QuestScript Function EventScript()
    Quest owner = Self as Quest
    Return owner as CB02_QuestScript
EndFunction

Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(0)
    ResetEventObjective(1)
    ResetEventObjective(2)
    ResetEventObjective(3)
    ResetEventObjective(4)
    ResetEventObjective(11)
    ResetEventObjective(100)
    ResetEventObjective(101)
    ResetEventObjective(110)
    ResetEventObjective(200)
    ResetEventObjective(300)
EndFunction

Function CompleteOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function AddPlayerToEventCollection()
    Actor playerRef = Game.GetPlayer()
    If Alias_players != None && playerRef != None && Alias_players.Find(playerRef) < 0
        Alias_players.AddRef(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0000_Item_00()
    ResetEventObjectives()
    AddPlayerToEventCollection()
    SetObjectiveDisplayed(0, True, True)
    SetObjectiveDisplayed(100, True, True)
    SetObjectiveDisplayed(101, True)
EndFunction

Function Fragment_Stage_0010_Item_00()
    AddPlayerToEventCollection()
    CB02_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.BeginRound()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    CB02_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.HandleRoundEnd()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    ; Late join is closed, so nobody is still waiting for the Mash to begin.
    CompleteOpenObjective(100)
EndFunction

Function Fragment_Stage_0900_Item_00()
    CompleteOpenObjective(100)
    CompleteOpenObjective(101)
    CompleteOpenObjective(110)
    CompleteOpenObjective(200)
    CompleteOpenObjective(300)
    CB02_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.FinishEvent()
    EndIf
EndFunction

Function Fragment_Stage_0999_Item_00()
    CB02_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.CleanupEvent()
    EndIf
    Stop()
EndFunction
