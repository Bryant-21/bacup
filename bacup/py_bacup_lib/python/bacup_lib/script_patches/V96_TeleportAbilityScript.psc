Event OnEffectStart(Actor akTarget, Actor akCaster)
    selfActor = akTarget
    TeleportManagerScript = V96_1 as v96_1_vaultmissionquestscript_telemgr
    If TeleportManagerScript == None
        Return
    EndIf

    ObjectReference linkedTrigger = akTarget.GetLinkedRef(V96_1_TeleportTriggerLinkKeyword)
    If linkedTrigger == None
        Return
    EndIf

    If akTarget.HasMagicEffect(V96_1_SuppressionEffect_ScriptSpell) || akTarget.HasMagicEffect(V96_1_SuppressionEffect_Weapon)
        Return
    EndIf

    TriggerBoundsDatum[] bounds
    If linkedTrigger == TeleportManagerScript.V96_1_EWS_MainframeTeleportTrigger.GetReference()
        teleportTriggerGroupIndex = 0
        bounds = TeleportManagerScript.MainframeTeleportTriggerBoundsData
    ElseIf linkedTrigger == TeleportManagerScript.V96_1_EWS_AtriumTeleportTrigger.GetReference()
        teleportTriggerGroupIndex = 1
        bounds = TeleportManagerScript.AtriumTeleportTriggerBoundsData
    ElseIf linkedTrigger == TeleportManagerScript.V96_1_EWS_MainframeControlTeleportTrigger.GetReference()
        teleportTriggerGroupIndex = 2
        bounds = TeleportManagerScript.MainframeControlTeleportTriggerBoundsData
    Else
        Return
    EndIf

    If bounds.Length == 0
        Return
    EndIf

    Int attempts = TeleportManagerScript.CONST_MaxTeleportAttempts
    If attempts < 1
        attempts = 1
    EndIf

    TriggerBoundsDatum datum
    Int tries = 0
    While tries < attempts
        datum = bounds[Utility.RandomInt(0, bounds.Length - 1)]
        If datum.triggerRef != None
            tries = attempts
        Else
            tries += 1
        EndIf
    EndWhile

    If datum.triggerRef == None
        Return
    EndIf

    Float destX = Utility.RandomFloat(datum.triggerMinX, datum.triggerMaxX)
    Float destY = Utility.RandomFloat(datum.triggerMinY, datum.triggerMaxY)
    Float destZ = Utility.RandomFloat(datum.triggerMinZ, datum.triggerMaxZ)

    If crV96TeleportExplosion
        akTarget.PlaceAtMe(crV96TeleportExplosion)
    EndIf
    akTarget.MoveTo(datum.triggerRef, destX, destY, destZ)
    If crV96TeleportExplosion
        akTarget.PlaceAtMe(crV96TeleportExplosion)
    EndIf
EndEvent
