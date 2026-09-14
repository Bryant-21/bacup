Event OnLoad()
    RequestOverseerQuestStart()
    PrepareQuestObjects()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    If CacheQuest != None && !CacheQuest.IsRunning() && !CacheQuest.IsCompleted()
        CacheQuest.Start()
    EndIf
    RequestOverseerQuestStart()
    PrepareQuestObjects()
EndEvent

; MQ_Overseer is event-scoped, so it can only be started through its Story
; Manager node. Fallout 76 fired that event from a Player Connect quest that has
; no Fallout 4 equivalent, so an Overseer cache does it instead.
Function RequestOverseerQuestStart()
    Quest overseerQuest = Game.GetFormFromFile(0x004E49D9, "SeventySix.esm") as Quest
    If overseerQuest == None || overseerQuest.IsRunning() || overseerQuest.IsCompleted()
        Return
    EndIf

    Keyword startKeyword = Game.GetFormFromFile(0x004E49E5, "SeventySix.esm") as Keyword
    If startKeyword != None
        startKeyword.SendStoryEventAndWait()
    EndIf
EndFunction

Function PrepareQuestObjects()
    Quest overseerQuest = Game.GetFormFromFile(0x004E49D9, "SeventySix.esm") as Quest
    If overseerQuest == None || !overseerQuest.IsRunning()
        Return
    EndIf

    PrepareQuestObject(overseerQuest, 0x004E49F6, 56)
    PrepareQuestObject(overseerQuest, 0x001389EC, 72)
    PrepareQuestObject(overseerQuest, 0x004E49FD, 57)
    PrepareQuestObject(overseerQuest, 0x004E49F8, 58)
    PrepareQuestObject(overseerQuest, 0x004EC62C, 59)
    PrepareQuestObject(overseerQuest, 0x004EC62E, 60)
    PrepareQuestObject(overseerQuest, 0x004EC62D, 61)
    PrepareQuestObject(overseerQuest, 0x004E49FC, 62)
    PrepareQuestObject(overseerQuest, 0x004EC630, 63)
    PrepareQuestObject(overseerQuest, 0x004E49F9, 64)
    PrepareQuestObject(overseerQuest, 0x004E49F4, 65)
    PrepareQuestObject(overseerQuest, 0x004EC62F, 66)
    PrepareQuestObject(overseerQuest, 0x003D1138, 74)
    PrepareQuestObject(overseerQuest, 0x004E49FB, 67)
    PrepareQuestObject(overseerQuest, 0x004E49F7, 68)
    PrepareQuestObject(overseerQuest, 0x004E49F3, 69)
    PrepareQuestObject(overseerQuest, 0x00528081, 70)
    PrepareQuestObject(overseerQuest, 0x00528086, 71)
EndFunction

Function PrepareQuestObject(Quest overseerQuest, Int holotapeFormId, Int aliasId)
    Holotape holotapeBase = Game.GetFormFromFile(holotapeFormId, "SeventySix.esm") as Holotape
    If holotapeBase == None || GetItemCount(holotapeBase) == 0
        Return
    EndIf

    ReferenceAlias questObjectAlias = overseerQuest.GetAlias(aliasId) as ReferenceAlias
    If questObjectAlias == None || questObjectAlias.GetReference() != None
        Return
    EndIf

    ObjectReference itemReference = PlaceAtMe(holotapeBase, 1, false, true, false)
    If itemReference == None
        Return
    EndIf
    itemReference.Enable()

    RemoveItem(holotapeBase, 1, true)
    questObjectAlias.ForceRefTo(itemReference)
    AddItem(itemReference, 1, true)
EndFunction
