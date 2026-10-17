; Tales Diseases.cpp runs this script's behavior natively for every disease (and Tales
; Sustenance for hunger/thirst), so the Papyrus body only serves the four non-disease
; bindings Tales does not track: the BURN Abraxo anti-armour effects (stingwing spit
; 80108D/83D828, nuke weather 853E03/853E04). Their data has only FireOnStart entries
; and a finish message, so no timer is started.
Bool Function IsAbraxoArmourEffect()
    Form effect = GetBaseObject()
    If effect == None
        Return False
    EndIf
    Return effect == Game.GetFormFromFile(0x0080108D, "SeventySix.esm") || effect == Game.GetFormFromFile(0x0083D828, "SeventySix.esm") || effect == Game.GetFormFromFile(0x00853E03, "SeventySix.esm") || effect == Game.GetFormFromFile(0x00853E04, "SeventySix.esm")
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget == None || akTarget != Game.GetPlayer() || !IsAbraxoArmourEffect()
        Return
    EndIf
    TargetPlayer = akTarget
    Int index = 0
    While PeriodicEffectData != None && index < PeriodicEffectData.Length
        PeriodicEffectDatum datum = PeriodicEffectData[index]
        If datum != None && datum.FireOnStart
            If datum.MessageToDisplay != None
                datum.MessageToDisplay.Show()
            EndIf
            If datum.SpellToCast != None
                datum.SpellToCast.Cast(akTarget, akTarget)
            EndIf
        EndIf
        index += 1
    EndWhile
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If TargetPlayer == None
        Return
    EndIf
    TargetPlayer = None
    If MessageToDisplayOnFinish != None
        MessageToDisplayOnFinish.Show()
    EndIf
EndEvent
