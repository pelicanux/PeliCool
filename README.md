# PeliCool

Personalize a tela LCD e a iluminação do seu water cooler no Linux com imagens,
GIFs e sensores do sistema. Projeto independente e de código aberto, sem vínculo
com o fabricante. Não controla a velocidade das fans ou da bomba.

O módulo atual atende ao **DeepCool MYSTIQUE**, com suporte confirmado no
**MYSTIQUE 360** USB `3633:0009`. Outros modelos precisam de um módulo compatível.

## Baixar e abrir

Baixe **[PeliCool.AppImage](https://github.com/pelicanux/PeliCool/releases/latest/download/PeliCool.AppImage)**
na seção [Releases](https://github.com/pelicanux/PeliCool/releases).
O pacote é para Linux **x86_64**.

Permita a execução nas propriedades do arquivo ou use:

```sh
chmod +x PeliCool.AppImage
./PeliCool.AppImage
```

Se sua distribuição não tiver FUSE disponível:

```sh
APPIMAGE_EXTRACT_AND_RUN=1 ./PeliCool.AppImage
```

A versão do aplicativo aparece em **Configurações → Sobre**.

## Acesso USB sem root

Na primeira abertura, o PeliCool oferece **Instalar regra udev** se faltar a
regra ou o acesso USB. Clique no botão e autorize na janela de autenticação do
sistema. Somente essa operação precisa de privilégios; abra o programa normalmente.

Se fechar o aviso ou cancelar a senha, poderá tentar novamente em
**Configurações → Acesso USB sem root**. Nessa seção também ficam **Remover regra
udev** e **Verificar acesso**. Regras alteradas manualmente são preservadas.
Pode ser necessário reconectar o USB após mudar as permissões.

A autenticação gráfica usa polkit/pkexec. Como alternativa:

```sh
sudo ./PeliCool.AppImage permissions install
# Para remover somente a regra gerenciada:
sudo ./PeliCool.AppImage permissions remove
```

## Instalar e iniciar com o sistema

Em **Configurações**, escolha **Instalar AppImage** para copiar o programa para
`~/.local/share/PeliCool/bin/PeliCool.AppImage` e criar um atalho no menu.
A instalação do aplicativo é por usuário e não exige sudo.

Ative **Iniciar com o sistema, minimizado** para restaurar a última imagem ou GIF
no login. Não é necessário abrir a janela ou trocar a mídia manualmente. O
programa seleciona o arquivo já salvo no display, sem reenviar a coleção.

Ao fechar a janela, você pode sair ou minimizar. Marque **Lembrar esta opção**
para minimizar sem perguntar novamente; altere essa preferência nas configurações.
Sem bandeja de sistema, a janela continua acessível pelo painel ou pelas atividades.

## Adicionar imagens e GIFs

1. Clique em **+** e escolha o arquivo.
2. Ajuste o recorte e o zoom no editor.
3. Confirme **Salvar no display** e acompanhe a barra de progresso.
4. Clique nas miniaturas para trocar instantaneamente. O botão direito permite remover.

O módulo atual permite até **cinco imagens e cinco GIFs**. Se um tipo estiver
cheio, remova um item para adicionar outro. A conversão adapta resolução, tamanho
e quantidade de quadros, preservando o original. Arquivos complexos podem perder
um pouco de detalhe ou fluidez para caber nos limites do display.

O slideshow alterna somente o tipo selecionado: imagens com imagens, GIFs com
GIFs. Com um único item do tipo, ele permanece exibido. As animações completam
seus ciclos antes da troca.

Em **Orientação da mídia**, 0° corresponde à marca do bloco em cima e legível.
Mude a posição e clique em **Aplicar tela e LEDs**. Para imagens e GIFs, mudar
a orientação prepara e envia novamente a coleção. **Editar enquadramento**
permite escolher recorte preenchido ou imagem inteira com bordas.

As mídias que devem permanecer precisam estar no catálogo local do PeliCool.
O aplicativo não baixa arquivos salvos no display por outro programa ou computador.

## Sensores e iluminação

- **Monitoring:** presets nativos com temperatura, frequência e outros sensores.
- **Recording:** gráficos nativos de monitoramento.
- **Media:** imagens e GIFs salvos no display.

Selecione os sensores disponíveis e aplique o preset. O monitoramento e o
slideshow continuam funcionando com a janela minimizada.

Os LEDs podem acompanhar a mídia, receber o sinal ARGB da placa-mãe ou usar o
alerta térmico nativo. Na sincronização com a placa-mãe, a integração opcional
com **OpenRGB** reaplica um perfil salvo pela CLI ou usa o SDK Server local.
O OpenRGB precisa estar instalado separadamente; configure a comunicação nas
configurações do PeliCool.

## Configurações e remoção

As preferências, os originais e o catálogo ficam organizados em
`~/.config/PeliCool/`, com subpastas por modelo. O tema e o comportamento ao
fechar são lembrados entre aberturas.

Use **Configurações → Desinstalar / limpar dados** para remover a instalação.
A limpeza das configurações e mídias locais é opcional. Seus arquivos originais
fora das pastas do PeliCool e os perfis do OpenRGB são preservados. A regra udev
é uma configuração do sistema e tem sua própria opção de remoção.

## Diagnóstico para novos dispositivos

Em **Configurações → Ajudar com novos dispositivos**, escolha **Coletar diagnóstico
local** e autorize a coleta. **Abrir pasta do diagnóstico** mostra os arquivos
para você revisar e anexar manualmente a uma issue.

O relatório não inclui hostname, usuário, IP, MAC, números de série, imagens
ou arquivos pessoais. Não usa rede nem envia comandos ao display. A coleta
também pode ser feita sem instalar o aplicativo:

```sh
./PeliCool.AppImage diagnose
# Ou pelo coletor independente:
sh scripts/pelicool-diagnose.sh
```

O relatório ajuda a identificar o aparelho; enviar os dados não garante suporte
automático. Novos modelos podem ser integrados por módulos e pull requests.

## Compilar

O aplicativo e as ferramentas são escritos em Rust. Instale o toolchain Rust,
pkg-config e as bibliotecas de desenvolvimento de udev, X11, Wayland, xkbcommon
(incluindo xkbcommon-x11) e OpenGL da sua distribuição. Depois execute:

```sh
cargo build --locked
./target/debug/pelicool gui
```

Para gerar o AppImage:

```sh
bash scripts/pelicool-appimage.sh
```

O arquivo será criado em `dist/PeliCool.AppImage`.

## Licença

Código sob licença MIT. Veja [LICENSE](LICENSE), os
[avisos dos componentes](THIRD-PARTY-NOTICES.md) e o comando `pelicool licenses`.
