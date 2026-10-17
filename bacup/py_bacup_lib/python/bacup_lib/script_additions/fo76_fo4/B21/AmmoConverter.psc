Scriptname B21:AmmoConverter Hidden
{Single-player AmmoPoints bank for the converted FO76 Ammo Converter terminal.

The stripped FO76 terminal fragments call Trade with the ammo form, the
converter's per-batch round count and per-batch points globals, and how many
batches the menu row stands for. Points live on the terminal reference, the
result code goes into the confirmation actor value that the Confirmation
terminal's text conditions read.}

Int Function ConfirmInsufficientPoints() global
    Return 0
EndFunction

Int Function ConfirmBuyComplete() global
    Return 1
EndFunction

Int Function ConfirmInsufficientAmmo() global
    Return 10
EndFunction

Int Function ConfirmSellComplete() global
    Return 11
EndFunction

Int Function ConfirmPointsAtMaximum() global
    Return 12
EndFunction

Float Function MaximumPoints() global
    Return 100000.0
EndFunction

ActorValue Function PointsStoredValue() global
    Return Game.GetFormFromFile(0x00586A9B, "SeventySix.esm") as ActorValue
EndFunction

ActorValue Function ConfirmationValue() global
    Return Game.GetFormFromFile(0x00586A9C, "SeventySix.esm") as ActorValue
EndFunction

Function Trade(ObjectReference akMachine, Form akAmmo, GlobalVariable akRoundsPerBatch, GlobalVariable akPointsPerBatch, Int aiBatches, Bool abBuy) global
    ActorValue pointsValue = PointsStoredValue()
    ActorValue confirmation = ConfirmationValue()
    If akMachine == None || akAmmo == None || akRoundsPerBatch == None || akPointsPerBatch == None || pointsValue == None || confirmation == None || aiBatches <= 0
        Return
    EndIf

    Actor player = Game.GetPlayer()
    Int rounds = (akRoundsPerBatch.GetValue() as Int) * aiBatches
    Float points = (akPointsPerBatch.GetValue() as Int) * aiBatches
    Float stored = akMachine.GetValue(pointsValue)
    Int result

    If abBuy
        If stored < points
            result = ConfirmInsufficientPoints()
        Else
            akMachine.SetValue(pointsValue, stored - points)
            player.AddItem(akAmmo, rounds, True)
            result = ConfirmBuyComplete()
        EndIf
    Else
        If player.GetItemCount(akAmmo) < rounds
            result = ConfirmInsufficientAmmo()
        ElseIf stored + points > MaximumPoints()
            result = ConfirmPointsAtMaximum()
        Else
            player.RemoveItem(akAmmo, rounds, True)
            akMachine.SetValue(pointsValue, stored + points)
            result = ConfirmSellComplete()
        EndIf
    EndIf

    akMachine.SetValue(confirmation, result as Float)
    Debug.Notification("AmmoPoints: " + (akMachine.GetValue(pointsValue) as Int) + " / " + (MaximumPoints() as Int))
EndFunction
