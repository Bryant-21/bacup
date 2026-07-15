; Keep the FO76 perk-sensitive activation prompt while restoring the inherited
; physical-hit initialization that the child OnLoad otherwise replaces.

Event OnLoad()
    parent.OnLoad()
    Actor playerRef = Game.GetPlayer()
    RegisterForRemoteEvent(playerRef, "OnPerkAdded")
    RegisterForRemoteEvent(playerRef, "OnPerkRemoved")
    CheckForActivationTextOverride()
EndEvent

Event OnUnload()
    Actor playerRef = Game.GetPlayer()
    UnregisterForRemoteEvent(playerRef, "OnPerkAdded")
    UnregisterForRemoteEvent(playerRef, "OnPerkRemoved")
EndEvent

Event Actor.OnPerkAdded(Actor akSender, Perk akPerk)
    CheckForActivationTextOverride()
EndEvent

Event Actor.OnPerkRemoved(Actor akSender, Perk akPerk)
    CheckForActivationTextOverride()
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    CheckForActivationTextOverride()
EndEvent

Bool Function HasDisarmPerks(Actor akActor)
    If akActor == None || RequiredDisarmPerks == None
        Return True
    EndIf

    Int perkIndex = 0
    While perkIndex < RequiredDisarmPerks.Length
        Perk requiredPerk = RequiredDisarmPerks[perkIndex]
        If requiredPerk != None && !akActor.HasPerk(requiredPerk)
            Return False
        EndIf
        perkIndex += 1
    EndWhile
    Return True
EndFunction

Function CheckForActivationTextOverride()
    Actor playerRef = Game.GetPlayer()
    If IsDestroyed() == False
        If NoDisarm
            BlockActivation(True, True)
        ElseIf HasDisarmPerks(playerRef)
            BlockActivation(False, False)
            SetActivateTextOverride(TrapMessageDisarm)
        Else
            BlockActivation(True, False)
        EndIf
    Else
        BlockActivation(False, False)
    EndIf
EndFunction
