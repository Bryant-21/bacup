Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || akActionRef != playerRef || RSVP01_Book_LORE_Newsletter == None
        Return
    EndIf
    If playerRef.GetItemCount(RSVP01_Book_LORE_Newsletter) == 0
        playerRef.AddItem(RSVP01_Book_LORE_Newsletter, 1)
    EndIf
    Quest masterQuest = Game.GetFormFromFile(0x0050A2EE, "SeventySix.esm") as Quest
    If masterQuest != None && masterQuest.IsRunning() && !masterQuest.IsStageDone(200)
        masterQuest.SetStage(200)
    EndIf
EndEvent
