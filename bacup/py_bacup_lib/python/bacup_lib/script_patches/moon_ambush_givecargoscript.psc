Function PlayInteractionSound(Actor player, Sound soundToPlay)
    If soundToPlay == None
        Return
    EndIf

    If MOON_Ambush_Keyword_SoundCooldown == None || !player.HasKeyword(MOON_Ambush_Keyword_SoundCooldown)
        soundToPlay.Play(Self)
        If MOON_Ambush_SPLL_InteractionSoundCooldown != None
            player.AddSpell(MOON_Ambush_SPLL_InteractionSoundCooldown, False)
        EndIf
    EndIf
EndFunction

Event OnInit()
    numberOfCargoRemaining = maxNumberOfCargo
EndEvent

Event OnActivate(ObjectReference akActionRef)
    Actor player = akActionRef as Actor
    If player == None || player != Game.GetPlayer()
        Return
    EndIf

    Int remainingCapacity = numberOfItemsPlayerCanCarry - player.GetItemCount(objectToGivePlayer)
    If remainingCapacity <= 0 || numberOfCargoRemaining <= 0
        If MessageWhenUnableToTakeCargo != None
            MessageWhenUnableToTakeCargo.Show()
        EndIf
        PlayInteractionSound(player, SoundWhenUnableToTakeCargo)
        Return
    EndIf

    Int cargoToGive = numberOfItemsToGive
    If cargoToGive > remainingCapacity
        cargoToGive = remainingCapacity
    EndIf
    If cargoToGive > numberOfCargoRemaining
        cargoToGive = numberOfCargoRemaining
    EndIf

    player.AddItem(objectToGivePlayer, cargoToGive, False)
    numberOfCargoRemaining -= cargoToGive
    If MessageWhenAbleToTakeCargo != None
        MessageWhenAbleToTakeCargo.Show()
    EndIf
    PlayInteractionSound(player, SoundWhenAbleToTakeCargo)
EndEvent
