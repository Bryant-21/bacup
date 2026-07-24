Event OnQuestInit()
    Actor player = Game.GetPlayer()
    If player
        ; FO4 Holotape forms cannot send OnHolotapePlay remotely. Use the
        ; player's supported inventory event as the single-player trigger.
        RegisterForRemoteEvent(player, "OnItemAdded")
    EndIf
EndEvent

Event ObjectReference.OnHolotapePlay(ObjectReference akSender, ObjectReference akTerminalRef)
    Holotape playedTape = GetPlayedHolotape(akSender)
    ProcessHolotape(playedTape, akSender)
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akSender != Game.GetPlayer() || aiItemCount <= 0
        Return
    EndIf

    ObjectReference eventRef = akItemReference
    If !eventRef
        eventRef = akSender
    EndIf
    ProcessHolotape(akBaseItem as Holotape, eventRef)
EndEvent

Function ProcessHolotape(Holotape playedTape, ObjectReference eventRef)
    If !playedTape
        Return
    EndIf

    Int index = 0
    While HolotapeData && index < HolotapeData.Length
        If HolotapeData[index].TriggeringTape == playedTape
            ; FO4 exposes neither a base-form play event nor playback-complete,
            ; so FO76 end triggers fall back to the supported pickup event.
            ApplyHolotapeDatum(index, eventRef)
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

Function ApplyHolotapeDatum(Int index, ObjectReference akSender)
    Actor player = Game.GetPlayer()
    If HolotapeData[index].ValueToSetOnListen
        player.SetValue(HolotapeData[index].ValueToSetOnListen, HolotapeData[index].OnListenActorValueNewValue)
    EndIf
    If HolotapeData[index].ValueToSetOnListen02
        player.SetValue(HolotapeData[index].ValueToSetOnListen02, HolotapeData[index].OnListenActorValueNewValue02)
    EndIf
    If HolotapeData[index].StoryEventKeyword
        HolotapeData[index].StoryEventKeyword.SendStoryEvent(akRef1 = akSender, aiValue1 = HolotapeData[index].StoryEventValueToSend)
    EndIf
    If HolotapeData[index].QuestToStart && !HolotapeData[index].QuestToStart.IsRunning()
        HolotapeData[index].QuestToStart.Start()
    EndIf
EndFunction
