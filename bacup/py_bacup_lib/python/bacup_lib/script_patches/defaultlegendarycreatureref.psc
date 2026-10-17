; FO76 forced these bosses legendary on the server. Tales applies a legendary rank from
; B21_LegendaryCreatureRank, the same contract DefaultForceLegendaryAlias uses.
Event OnLoad()
    ApplyForcedLegendaryRank()
EndEvent

Function ApplyForcedLegendaryRank()
    Actor target = (Self as ObjectReference) as Actor
    If !bForceLegendary || target == None || target.IsDead()
        Return
    EndIf
    If !Game.IsPluginInstalled("B21_TalesFromAppalachia.esm")
        Return
    EndIf
    ActorValue rankValue = Game.GetFormFromFile(0x00FFD809, "B21_TalesFromAppalachia.esm") as ActorValue
    If rankValue == None || target.GetValue(rankValue) > 0.0
        Return
    EndIf
    Int rank = LegendaryRank
    If rank < 1
        rank = 1
    ElseIf rank > 5
        rank = 5
    EndIf
    target.SetValue(rankValue, rank as Float)
EndFunction
