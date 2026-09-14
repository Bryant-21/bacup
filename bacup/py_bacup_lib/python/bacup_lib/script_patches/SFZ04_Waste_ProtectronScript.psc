Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    If akSenderRef == None
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    ObjectReference playerRef
    If Alias_SFZ04Player != None
        playerRef = Alias_SFZ04Player.GetReference()
    EndIf
    If owningQuest != None && owningQuest.IsRunning() && playerRef != None
        If SFZ04_Waste_Core != None && akSenderRef.GetItemCount(SFZ04_Waste_Core) == 0
            akSenderRef.AddItem(SFZ04_Waste_Core, 1, True)
        EndIf
    EndIf

    If Alias_SpawnedProtectrons != None && Alias_SpawnedProtectrons.Find(akSenderRef) >= 0
        Alias_SpawnedProtectrons.RemoveRef(akSenderRef)
    EndIf
EndEvent
