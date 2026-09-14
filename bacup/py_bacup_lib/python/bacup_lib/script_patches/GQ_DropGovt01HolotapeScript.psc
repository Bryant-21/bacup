Event OnInit()
    RequestGovernmentDropStart(GetContainer())
EndEvent

Event OnContainerChanged(ObjectReference akNewContainer, ObjectReference akOldContainer)
    RequestGovernmentDropStart(akNewContainer)
EndEvent

Function RequestGovernmentDropStart(ObjectReference akContainer)
    Actor playerRef = Game.GetPlayer()
    If akContainer != playerRef || GQ_DropGovtIntroKeyword == None
        Return
    EndIf

    Quest governmentDrop = Game.GetFormFromFile(0x006F3C, "SeventySix.esm") as Quest
    If governmentDrop == None || governmentDrop.IsRunning() || governmentDrop.IsCompleted()
        Return
    EndIf

    Bool started = GQ_DropGovtIntroKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    Debug.Trace("[GQ_DropGovt01HolotapeScript] acquisition start sent; started=" + started + " running=" + governmentDrop.IsRunning())
EndFunction
