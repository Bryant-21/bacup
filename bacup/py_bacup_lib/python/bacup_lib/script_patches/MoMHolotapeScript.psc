Event OnQuestInit()
    Actor player = Game.GetPlayer()
    If player
        RegisterForRemoteEvent(player, "OnItemAdded")
    EndIf
EndEvent

Event ObjectReference.OnHolotapePlay(ObjectReference akSender, ObjectReference akTerminalRef)
    ProcessHolotape(GetPlayedHolotape(akSender))
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akSender == Game.GetPlayer() && aiItemCount > 0
        ProcessHolotape(akBaseItem as Holotape)
    EndIf
EndEvent

Function ProcessHolotape(Holotape playedTape)
    Int index = 0
    While playedTape && MoMHolotapeData && index < MoMHolotapeData.Length
        If MoMHolotapeData[index].MoMHolotape == playedTape && MoMMaster
            MoMMaster.SetStage(MoMHolotapeData[index].MoMQuestStageToSet)
        EndIf
        index += 1
    EndWhile
EndFunction

Holotape Function GetPlayedHolotape(ObjectReference akSender)
    If akSender
        Return akSender.GetBaseObject() as Holotape
    EndIf
    Return None
EndFunction
