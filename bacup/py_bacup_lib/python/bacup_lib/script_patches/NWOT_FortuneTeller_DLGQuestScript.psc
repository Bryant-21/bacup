; The fortune teller reads the cards for NWOT_Fortune_CapsCost caps: one Fortunes group (positive or negative major
; arcana) is drawn, the player gets a random note from its list with a lucky flavor filled in plus its 30-minute
; FortuneSpell, and NWOT_FortuneSpell_Cooldown gates the next reading.
Function GiveFortune()
    Actor player = Game.GetPlayer()
    Int cost = 0
    If NWOT_Fortune_CapsCost != None
        cost = NWOT_Fortune_CapsCost.GetValueInt()
    EndIf
    If Fortunes == None || Fortunes.Length == 0 || (cost > 0 && player.GetItemCount(Caps001) < cost)
        Return
    EndIf
    If cost > 0
        player.RemoveItem(Caps001, cost)
    EndIf
    FortuneGroup fortune = Fortunes[Utility.RandomInt(0, Fortunes.Length - 1)]
    If fortune.FortuneList != None && fortune.FortuneList.GetSize() > 0
        Form noteBase = fortune.FortuneList.GetAt(Utility.RandomInt(0, fortune.FortuneList.GetSize() - 1))
        ObjectReference note = player.PlaceAtMe(noteBase, 1, False, True)
        If note != None
            NWOT_Fortune_NoteScript fortuneNote = note as NWOT_Fortune_NoteScript
            If fortuneNote != None
                fortuneNote.SetLuckyFlavor()
            EndIf
            player.AddItem(note, 1)
        EndIf
    EndIf
    If fortune.FortuneSpell != None
        fortune.FortuneSpell.Cast(player, player)
    EndIf
    If NWOT_FortuneSpell_Cooldown != None
        NWOT_FortuneSpell_Cooldown.Cast(player, player)
    EndIf
EndFunction
