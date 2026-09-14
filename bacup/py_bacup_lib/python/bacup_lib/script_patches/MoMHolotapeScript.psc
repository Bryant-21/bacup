Event OnQuestInit()
	ReconcileRuntimeRegistrations()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer()
		ReconcileRuntimeRegistrations()
	EndIf
EndEvent

Event OnQuestShutdown()
	ClearRuntimeRegistrations()
EndEvent

Function ClearRuntimeRegistrations()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnItemAdded")
		UnregisterForRemoteEvent(playerRef, "OnHolotapePlay")
		UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	RemoveAllInventoryEventFilters()
EndFunction

Function ReconcileRuntimeRegistrations()
	ClearRuntimeRegistrations()
	If !IsRunning() || IsCompleted()
		Return
	EndIf

	Int index = 0
	If MoMHolotapeData != None
		While index < MoMHolotapeData.Length
			Holotape holotapeFilter = MoMHolotapeData[index].MoMHolotape
			Int firstIndex = 0
			While firstIndex < index && MoMHolotapeData[firstIndex].MoMHolotape != holotapeFilter
				firstIndex += 1
			EndWhile
			If holotapeFilter != None && firstIndex == index
				AddInventoryEventFilter(holotapeFilter)
			EndIf
			index += 1
		EndWhile
	EndIf

	Actor player = Game.GetPlayer()
	If player != None
		RegisterForRemoteEvent(player, "OnItemAdded")
		RegisterForRemoteEvent(player, "OnPlayerLoadGame")
	EndIf
EndFunction

Event ObjectReference.OnHolotapePlay(ObjectReference akSender, ObjectReference akTerminalRef)
    If akSender == None
        Return
    EndIf
    If !IsRunning() || IsCompleted()
        UnregisterForRemoteEvent(akSender, "OnHolotapePlay")
        Return
    EndIf

    Holotape playedTape = GetPlayedHolotape(akSender)
    ProcessHolotape(playedTape)
    If IsHolotapeProgressComplete(playedTape)
        UnregisterForRemoteEvent(akSender, "OnHolotapePlay")
    EndIf
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If IsRunning() && !IsCompleted() && akSender == Game.GetPlayer() && aiItemCount > 0 && akItemReference != None
        RegisterForRemoteEvent(akItemReference, "OnHolotapePlay")
    EndIf
EndEvent

Function ProcessHolotape(Holotape playedTape)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If playedTape == None || MoMHolotapeData == None || masterScript == None || masterScript.MoMQuestList == None
        Return
    EndIf

    Int index = 0
    While index < MoMHolotapeData.Length
        If MoMHolotapeData[index].MoMHolotape == playedTape && MoMHolotapeData[index].MoMQuestName != ""
            Int questIndex = 0
            While questIndex < masterScript.MoMQuestList.Length
                If masterScript.MoMQuestList[questIndex].MoMQuestName == MoMHolotapeData[index].MoMQuestName
                    Quest targetQuest = masterScript.MoMQuestList[questIndex].MoMQuest
                    If targetQuest != None && !targetQuest.IsRunning() && !targetQuest.IsCompleted()
                        Keyword startKeyword = masterScript.MoMQuestList[questIndex].MoMQuestKeyword
                        If startKeyword != None
                            startKeyword.SendStoryEventAndWait(None, Game.GetPlayer())
                        EndIf
                    EndIf
                    If targetQuest != None && targetQuest.IsRunning() && !targetQuest.IsStageDone(MoMHolotapeData[index].MoMQuestStageToSet)
                        targetQuest.SetStage(MoMHolotapeData[index].MoMQuestStageToSet)
                    EndIf
                    Return
                EndIf
                questIndex += 1
            EndWhile
        EndIf
        index += 1
    EndWhile
EndFunction

Bool Function IsHolotapeProgressComplete(Holotape playedTape)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If playedTape == None || MoMHolotapeData == None || masterScript == None || masterScript.MoMQuestList == None
        Return False
    EndIf

    Int index = 0
    While index < MoMHolotapeData.Length
        If MoMHolotapeData[index].MoMHolotape == playedTape && MoMHolotapeData[index].MoMQuestName != ""
            Int questIndex = 0
            While questIndex < masterScript.MoMQuestList.Length
                If masterScript.MoMQuestList[questIndex].MoMQuestName == MoMHolotapeData[index].MoMQuestName
                    Quest targetQuest = masterScript.MoMQuestList[questIndex].MoMQuest
                    Return targetQuest != None && (targetQuest.IsCompleted() || targetQuest.IsStageDone(MoMHolotapeData[index].MoMQuestStageToSet))
                EndIf
                questIndex += 1
            EndWhile
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Holotape Function GetPlayedHolotape(ObjectReference akSender)
    If akSender
        Return akSender.GetBaseObject() as Holotape
    EndIf
    Return None
EndFunction
