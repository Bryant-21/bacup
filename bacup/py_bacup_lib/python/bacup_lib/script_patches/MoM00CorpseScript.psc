Event OnLoad()
    Actor player = Game.GetPlayer()
    If player != None && MoM00 != None && !MoM00.IsCompleted()
        RegisterForDistanceLessThanEvent(Self, player, CONST_CorpseDistanceCheckDistance as Float)
    EndIf
EndEvent

Event OnUnload()
    Actor player = Game.GetPlayer()
    If player != None
        UnregisterForDistanceEvents(Self, player)
    EndIf
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    If akObj1 != Self || akObj2 != Game.GetPlayer()
        Return
    EndIf

    StartDiscoveryQuest()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        StartDiscoveryQuest()
    EndIf
EndEvent

Function StartDiscoveryQuest()
    If MoM00 == None || MoM00.IsCompleted()
        Return
    EndIf

    Actor player = Game.GetPlayer()
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If player == None || masterScript == None || masterScript.MoMQuestList == None || masterScript.MoMQuestList.Length <= 1
        Return
    EndIf

    If !MoM00.IsRunning()
        Keyword startKeyword = masterScript.MoMQuestList[1].MoMQuestKeyword
        If startKeyword != None
            startKeyword.SendStoryEventAndWait(None, Self, player)
        EndIf
    EndIf

    If MoM00.IsRunning()
        MoM00QuestScript questScript = MoM00 as MoM00QuestScript
        If questScript != None && questScript.MoM00Corpse != None
            questScript.MoM00Corpse.ForceRefTo(Self)
        EndIf
        If !MoM00.IsStageDone(20)
            MoM00.SetStage(20)
        EndIf
    EndIf
EndFunction
