; Tales publishes each player emote on its event bus; FO76 counted emotes performed near the Mothman statue.
B21:B21_TFA_PublicEventController Function EmoteBus()
    Return Game.GetFormFromFile(0xFFF017, "B21_TalesFromAppalachia.esm") as B21:B21_TFA_PublicEventController
EndFunction

Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    QuestScript = OwningQuest as Quests:E07_Mothman:QuestScript
    StatueRef = None
    B21:B21_TFA_PublicEventController bus = EmoteBus()
    If bus != None
        RegisterForCustomEvent(bus, "B21EmoteV1")
    EndIf
EndEvent

Event OnAliasShutdown()
    B21:B21_TFA_PublicEventController bus = EmoteBus()
    If bus != None
        UnregisterForCustomEvent(bus, "B21EmoteV1")
    EndIf
    StatueRef = None
EndEvent

Event B21:B21_TFA_PublicEventController.B21EmoteV1(B21:B21_TFA_PublicEventController akSender, Var[] akArgs)
    If akArgs.Length != 5 || (akArgs[0] as Int) != 1
        Return
    EndIf
    Actor emoter = akArgs[1] as Actor
    If emoter == None || emoter != Game.GetPlayer() || QuestScript == None
        Return
    EndIf
    If OwningQuest.IsStageDone(Stage_DanceRequested) && !OwningQuest.IsStageDone(QuestScript.Stage_DancesComplete) && NearStatue(emoter)
        QuestScript.Danced()
    EndIf
EndEvent

Bool Function NearStatue(Actor akPlayer)
    If StatueRef == None && Statue_Alias != None
        StatueRef = Statue_Alias.GetReference()
    EndIf
    Return StatueRef == None || akPlayer.GetDistance(StatueRef) <= AllowedDistanceFromStatue
EndFunction
