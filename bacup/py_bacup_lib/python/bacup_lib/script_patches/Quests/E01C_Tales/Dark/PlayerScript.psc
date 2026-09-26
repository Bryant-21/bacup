B21:B21_TFA_PublicEventController Function EmoteBus()
    Return Game.GetFormFromFile(0xFFF017, "B21_TalesFromAppalachia.esm") as B21:B21_TFA_PublicEventController
EndFunction

Function ArmEmoteListener()
    B21:B21_TFA_PublicEventController bus = EmoteBus()
    If bus != None
        RegisterForCustomEvent(bus, "B21EmoteV1")
    EndIf
EndFunction

Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    QS = OwningQuest as Quests:E01C_Tales:Dark:QuestScript
    PartyActionsCurr = 0
    CampfireRef = None
    ArmEmoteListener()
EndEvent

Event OnPlayerLoadGame(ObjectReference akSenderRef)
    ArmEmoteListener()
EndEvent

Event OnAliasShutdown()
    B21:B21_TFA_PublicEventController bus = EmoteBus()
    If bus != None
        UnregisterForCustomEvent(bus, "B21EmoteV1")
    EndIf
EndEvent

Event OnItemEquipped(ObjectReference akSenderRef, Form akBaseObject, ObjectReference akReference)
    If akBaseObject == None
        Return
    EndIf
    Bool isFood = ObjectTypeFood != None && akBaseObject.HasKeyword(ObjectTypeFood)
    Bool isDrink = ObjectTypeDrink != None && akBaseObject.HasKeyword(ObjectTypeDrink)
    If isFood || isDrink
        RecordPartyAction(akSenderRef)
    EndIf
EndEvent

Event B21:B21_TFA_PublicEventController.B21EmoteV1(B21:B21_TFA_PublicEventController akSender, Var[] akArgs)
    If akArgs.Length != 5
        Return
    EndIf
    Int aiVersion = akArgs[0] as Int
    Actor akPlayer = akArgs[1] as Actor
    String asPlugin = akArgs[2] as String
    Int aiSourceID = akArgs[3] as Int
    Int aiCategoryID = akArgs[4] as Int
    If aiVersion == 1 && asPlugin == "SeventySix.esm" && aiCategoryID == 22337 && (aiSourceID == 1114476 || aiSourceID == 5234353)
        RecordPartyAction(akPlayer)
    EndIf
EndEvent

Bool Function IsNearCampfire(ObjectReference akActor)
    If CampfireRef == None && Alias_Activator_Campfire != None
        CampfireRef = Alias_Activator_Campfire.GetReference()
    EndIf
    Return akActor != None && CampfireRef != None && akActor.GetDistance(CampfireRef) <= AllowedDistanceFromCampfire as Float
EndFunction

Function RecordPartyAction(ObjectReference akActor)
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
        QS = OwningQuest as Quests:E01C_Tales:Dark:QuestScript
    EndIf
    If !OwningQuest.IsStageDone(Stage_PartyBegin) || OwningQuest.IsStageDone(Stage_PartyEnd)
        Return
    EndIf
    If akActor != Game.GetPlayer() || !IsNearCampfire(akActor)
        Return
    EndIf
    PartyActionsCurr += 1
    Int required = 10
    If QS != None && QS.PartyActionsReq > 0
        required = QS.PartyActionsReq
    EndIf
    If PartyActionsCurr >= required
        OwningQuest.SetStage(Stage_PartyEnd)
    ElseIf OwningQuest.IsObjectiveDisplayed(Obj_Party)
        OwningQuest.SetObjectiveDisplayed(Obj_Party, True, True)
    EndIf
EndFunction
