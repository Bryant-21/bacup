Event OnQuestInit()
    Actor player = Game.GetPlayer()
    If player
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

Holotape Function GetPlayedHolotape(ObjectReference akSender)
    If akSender
        Return akSender.GetBaseObject() as Holotape
    EndIf
    Return None
EndFunction

Function ProcessHolotape(Holotape playedTape, ObjectReference eventRef)
    If !IsTriggeringTape(playedTape)
        Return
    EndIf

    Actor player = Game.GetPlayer()
    If pW05_MQ00_Completed && player.GetValue(pW05_MQ00_Completed) > 0.0
        Return
    EndIf
    If pW05_MQ00_CodeAV && player.GetValue(pW05_MQ00_CodeAV) < 0.0
        player.SetValue(pW05_MQ00_CodeAV, Utility.RandomInt(100000, 999999))
    EndIf

    Bool started = False
    If pW05_MQ_00P_StartKeyword
        started = pW05_MQ_00P_StartKeyword.SendStoryEventAndWait(akRef1 = eventRef)
    EndIf
    If !started && W05_MQ_00P && !W05_MQ_00P.IsRunning()
        W05_MQ_00P.Start()
    EndIf
EndFunction

Bool Function IsTriggeringTape(Holotape playedTape)
    If !playedTape
        Return False
    EndIf
    Return playedTape == Game.GetFormFromFile(0x00569C98, "SeventySix.esm") || playedTape == Game.GetFormFromFile(0x005852F0, "SeventySix.esm")
EndFunction
